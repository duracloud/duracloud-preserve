//! Local HTTP tests for folder concurrency and the final user update.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use app::sftpgo::sync_user_access;
use serde_json::{Value, json};
use sftpgo::{Error, FolderUpsert, SFTPGoClient, SFTPGoConfig, base_folders};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::Instant;

#[derive(Clone, Copy)]
enum Scenario {
    Normal,
    Forbidden,
    Transient {
        failures: usize,
        status: &'static str,
        retry_after: Option<&'static str>,
    },
    LostCreateResponse,
    StalledProbe,
    TransientProbe,
    StalledProbeBody,
    TruncatedProbeBody,
    MalformedProbe,
}

#[derive(Default)]
struct State {
    active: usize,
    peak: usize,
    active_writes: usize,
    peak_writes: usize,
    created: usize,
    updated: usize,
    user_updates: Vec<serde_json::Value>,
    finished_before_user_update: usize,
    probes: Vec<Instant>,
    writes: Vec<Instant>,
    /// Stored folders by name, as SFTPGo returns them (secret payload hidden).
    folders: HashMap<String, Value>,
}

/// Count each folder request until its response finishes or its task is cancelled.
struct InFlightRequest {
    state: Arc<Mutex<State>>,
    is_write: bool,
}

impl InFlightRequest {
    fn new(state: Arc<Mutex<State>>, is_write: bool) -> Self {
        {
            let mut state = state.lock().unwrap();
            state.active += 1;
            state.peak = state.peak.max(state.active);
            if is_write {
                state.active_writes += 1;
                state.peak_writes = state.peak_writes.max(state.active_writes);
            }
        }
        Self { state, is_write }
    }
}

impl Drop for InFlightRequest {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.active -= 1;
        if self.is_write {
            state.active_writes -= 1;
        }
    }
}

/// Store a folder body the way SFTPGo reports it back: encrypted, payload hidden.
fn hide_secret(mut folder: Value) -> Value {
    folder["filesystem"]["s3config"]["access_secret"] = json!({"status": "AES-256-GCM"});
    folder
}

struct Server {
    client: SFTPGoClient,
    state: Arc<Mutex<State>>,
    task: JoinHandle<()>,
}

impl Server {
    async fn start(fail_writes: bool) -> Self {
        Self::with_scenario(if fail_writes {
            Scenario::Forbidden
        } else {
            Scenario::Normal
        })
        .await
    }

    async fn with_scenario(scenario: Scenario) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let server_state = state.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (socket, _) = accepted.unwrap();
                        connections.spawn(serve(socket, server_state.clone(), scenario));
                    }
                    Some(result) = connections.join_next() => { result.unwrap(); }
                }
            }
        });
        let client = SFTPGoClient::new(
            reqwest::Client::builder()
                .no_proxy()
                .timeout(if matches!(scenario, Scenario::StalledProbe) {
                    Duration::from_secs(60)
                } else {
                    Duration::from_secs(1)
                })
                .build()
                .unwrap(),
            SFTPGoConfig {
                host,
                username: "admin".into(),
                password: "test".into(),
            },
        );
        Self {
            client,
            state,
            task,
        }
    }
}

impl Server {
    /// Pre-create alice's folders for `buckets` with the given access key.
    fn seed(&self, buckets: &[&str], access_key: &str) {
        let mut state = self.state.lock().unwrap();
        for folder in base_folders("alice", buckets, "us-east-1", access_key, "test-secret") {
            let value = hide_secret(serde_json::to_value(&folder).unwrap());
            state.folders.insert(folder.name, value);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(mut socket: TcpStream, state: Arc<Mutex<State>>, scenario: Scenario) {
    // Read one complete request per connection; responses explicitly close it.
    let mut data = Vec::new();
    let (header_end, content_length) = loop {
        let mut chunk = [0; 4096];
        let read = socket.read(&mut chunk).await.unwrap();
        if read == 0 {
            return;
        }
        data.extend_from_slice(&chunk[..read]);
        if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&data[..end]);
            let length = headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                .unwrap_or(0);
            break (end + 4, length);
        }
    };
    while data.len() < header_end + content_length {
        let mut chunk = [0; 4096];
        let read = socket.read(&mut chunk).await.unwrap();
        if read == 0 {
            return;
        }
        data.extend_from_slice(&chunk[..read]);
    }
    let headers = String::from_utf8_lossy(&data[..header_end]);
    let mut request = headers.lines().next().unwrap().split_whitespace();
    let method = request.next().unwrap();
    let path = request.next().unwrap();
    let mut status = "200 OK";
    let mut body = String::from("{}");
    let mut retry_after = None;
    let mut lose_response = false;
    let mut stall_body = false;
    let mut truncate_body = false;
    let _in_flight;
    if path.starts_with("/api/v2/users/") {
        if method == "GET" {
            body = r#"{"username":"alice"}"#.into();
        } else {
            assert_eq!(method, "PUT");
            let mut state = state.lock().unwrap();
            state.finished_before_user_update = state.created + state.updated;
            state
                .user_updates
                .push(serde_json::from_slice(&data[header_end..]).unwrap());
        }
    } else {
        assert!(path.starts_with("/api/v2/folders"));
        _in_flight = InFlightRequest::new(state.clone(), method != "GET");
        // Delay reads and writes so both contribute to observable concurrency.
        tokio::time::sleep(Duration::from_millis(20)).await;
        if method == "GET" {
            {
                let mut state = state.lock().unwrap();
                state.probes.push(Instant::now());
            }
            if matches!(scenario, Scenario::StalledProbe) {
                std::future::pending::<()>().await;
            }
            let state = state.lock().unwrap();
            let name = path.trim_start_matches("/api/v2/folders/");
            match state.folders.get(name) {
                Some(folder) => body = folder.to_string(),
                None => status = "404 Not Found",
            }
            if matches!(scenario, Scenario::TransientProbe) && state.probes.len() == 1 {
                status = "503 Service Unavailable";
                retry_after = Some("1");
            }
            stall_body = matches!(scenario, Scenario::StalledProbeBody) && state.probes.len() == 1;
            truncate_body = matches!(scenario, Scenario::TruncatedProbeBody);
            if matches!(scenario, Scenario::MalformedProbe) {
                status = "200 OK";
                body = "{invalid json".into();
            }
        } else {
            let mut state = state.lock().unwrap();
            state.writes.push(Instant::now());
            match scenario {
                Scenario::Forbidden => status = "403 Forbidden",
                Scenario::Transient {
                    failures,
                    status: failure_status,
                    retry_after: delay,
                } if state.writes.len() <= failures => {
                    status = failure_status;
                    retry_after = delay;
                }
                _ => {}
            }
            if status == "200 OK" {
                let folder: Value = serde_json::from_slice(&data[header_end..]).unwrap();
                let name = folder["name"].as_str().unwrap().to_owned();
                state.folders.insert(name, hide_secret(folder));
            }
            if status == "200 OK" && method == "POST" {
                state.created += 1;
                lose_response = matches!(scenario, Scenario::LostCreateResponse);
            } else if status == "200 OK" {
                assert_eq!(method, "PUT");
                state.updated += 1;
            }
        }
    }
    if lose_response {
        // Apply the creation, then withhold its response beyond the client's timeout.
        tokio::time::sleep(Duration::from_secs(2)).await;
        return;
    }
    let retry_header = retry_after
        .map(|delay| format!("Retry-After: {delay}\r\n"))
        .unwrap_or_default();
    if stall_body || truncate_body {
        // Send successful headers and only part of the advertised body.
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{{",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
        if stall_body {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        return;
    }
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{retry_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    // Concurrent requests may be cancelled after another folder fails.
    let _ = socket.write_all(response.as_bytes()).await;
}

#[tokio::test]
async fn folder_updates_overlap_with_a_limit_and_finish_before_user_update() {
    let server = Server::start(false).await;
    let buckets: Vec<String> = (0..20)
        .map(|index| format!("test-stack-bucket-{index}"))
        .collect();
    let names: Vec<&str> = buckets.iter().map(String::as_str).collect();
    // Odd buckets already exist with a rotated-out key, so they need rewriting.
    let odd: Vec<&str> = names.iter().copied().skip(1).step_by(2).collect();
    server.seed(&odd, "old-access");
    tokio::time::timeout(
        Duration::from_secs(5),
        sync_user_access(
            &server.client,
            "alice",
            &names,
            "us-east-1",
            "test-access",
            "test-secret",
        ),
    )
    .await
    .unwrap()
    .unwrap();

    let state = server.state.lock().unwrap();
    assert!(state.peak > 1, "folder requests must overlap");
    assert!(
        state.peak <= 8,
        "at most eight folder requests may be in flight"
    );
    assert!(state.peak_writes > 1, "folder writes must overlap");
    assert_eq!(state.active, 0);
    assert_eq!(state.active_writes, 0);
    assert_eq!(state.created, 10);
    assert_eq!(state.updated, 10);
    assert_eq!(state.finished_before_user_update, 20);
    assert_eq!(state.user_updates.len(), 1);
    let folders = state.user_updates[0]["virtual_folders"].as_array().unwrap();
    assert_eq!(folders.len(), 20);
    for (folder, bucket) in folders.iter().zip(&buckets) {
        assert_eq!(folder["virtual_path"], format!("/{bucket}"));
    }
}

#[tokio::test]
async fn unchanged_folders_are_not_rewritten() {
    let server = Server::start(false).await;
    let buckets = ["test-stack-bucket-0", "test-stack-bucket-1"];
    server.seed(&buckets, "test-access");
    tokio::time::timeout(
        Duration::from_secs(5),
        sync_user_access(
            &server.client,
            "alice",
            &buckets,
            "us-east-1",
            "test-access",
            "test-secret",
        ),
    )
    .await
    .unwrap()
    .unwrap();

    let state = server.state.lock().unwrap();
    assert_eq!(state.probes.len(), 2);
    assert_eq!(state.active, 0);
    assert!(state.writes.is_empty());
    assert_eq!(state.user_updates.len(), 1);
}

#[tokio::test]
async fn probe_body_timeout_retries_without_rewriting_matching_folder() {
    let server = Server::with_scenario(Scenario::StalledProbeBody).await;
    server.seed(&["test-stack-bucket-0"], "test-access");
    sync_one(&server).await.unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.probes.len(), 2);
    assert!(state.writes.is_empty());
    assert_eq!(state.user_updates.len(), 1);
}

#[tokio::test]
async fn truncated_probe_body_fails_without_writing() {
    let server = Server::with_scenario(Scenario::TruncatedProbeBody).await;
    server.seed(&["test-stack-bucket-0"], "test-access");
    assert!(matches!(sync_one(&server).await, Err(Error::Request(_))));
    let state = server.state.lock().unwrap();
    assert_eq!(state.probes.len(), 1);
    assert!(state.writes.is_empty());
    assert!(state.user_updates.is_empty());
}

#[tokio::test]
async fn malformed_folder_response_is_rewritten() {
    let server = Server::with_scenario(Scenario::MalformedProbe).await;
    let folder = base_folders(
        "alice",
        &["test-stack-bucket-0"],
        "us-east-1",
        "test-access",
        "test-secret",
    )
    .pop()
    .unwrap();
    assert_eq!(
        server.client.upsert_folder(&folder).await.unwrap(),
        FolderUpsert::Updated("unreadable")
    );
    let state = server.state.lock().unwrap();
    assert_eq!(state.probes.len(), 1);
    assert_eq!(state.created, 0);
    assert_eq!(state.updated, 1);
}

#[tokio::test]
async fn folder_failure_prevents_user_update() {
    let server = Server::start(true).await;
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        sync_user_access(
            &server.client,
            "alice",
            &["test-stack-bucket-0"],
            "us-east-1",
            "test-access",
            "test-secret",
        ),
    )
    .await
    .unwrap();

    assert!(matches!(
        result,
        Err(Error::Api {
            status: reqwest::StatusCode::FORBIDDEN,
            ..
        })
    ));
    assert!(server.state.lock().unwrap().user_updates.is_empty());
    assert_eq!(server.state.lock().unwrap().writes.len(), 1);
}

async fn sync_one(server: &Server) -> Result<(), Error> {
    tokio::time::timeout(
        Duration::from_secs(5),
        sync_user_access(
            &server.client,
            "alice",
            &["test-stack-bucket-0"],
            "us-east-1",
            "test-access",
            "test-secret",
        ),
    )
    .await
    .expect("retry operation should finish within its test budget")
}

#[tokio::test]
async fn transient_folder_failures_succeed_on_third_attempt() {
    let server = Server::with_scenario(Scenario::Transient {
        failures: 2,
        status: "503 Service Unavailable",
        retry_after: None,
    })
    .await;
    sync_one(&server).await.unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.probes.len(), 3);
    assert_eq!(state.writes.len(), 3);
    assert_eq!(state.created, 1);
    assert_eq!(state.user_updates.len(), 1);
}

#[tokio::test]
async fn transient_folder_failures_stop_after_three_attempts() {
    let server = Server::with_scenario(Scenario::Transient {
        failures: usize::MAX,
        status: "503 Service Unavailable",
        retry_after: None,
    })
    .await;
    assert!(matches!(
        sync_one(&server).await,
        Err(Error::Api {
            status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
            ..
        })
    ));
    let state = server.state.lock().unwrap();
    assert_eq!(state.writes.len(), 3);
    assert!(state.user_updates.is_empty());
}

#[tokio::test]
async fn rate_limit_retry_waits_for_retry_after() {
    let server = Server::with_scenario(Scenario::Transient {
        failures: 1,
        status: "429 Too Many Requests",
        retry_after: Some("1"),
    })
    .await;
    sync_one(&server).await.unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.writes.len(), 2);
    assert!(state.probes[1].duration_since(state.writes[0]) >= Duration::from_secs(1));
}

#[tokio::test]
async fn retry_after_exceeding_budget_stops_without_retrying_early() {
    let server = Server::with_scenario(Scenario::Transient {
        failures: 1,
        status: "429 Too Many Requests",
        retry_after: Some("120"),
    })
    .await;
    match sync_one(&server).await.unwrap_err() {
        Error::Api { retry_after, .. } => assert_eq!(retry_after, Some(Duration::from_secs(120))),
        error => panic!("unexpected error: {error}"),
    }
    let state = server.state.lock().unwrap();
    assert_eq!(state.writes.len(), 1);
    assert!(state.user_updates.is_empty());
}

#[tokio::test]
async fn lost_create_response_is_recognised_by_existence_check() {
    let server = Server::with_scenario(Scenario::LostCreateResponse).await;
    sync_one(&server).await.unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.probes.len(), 2);
    assert_eq!(state.created, 1);
    // The retry finds the folder it created and leaves it alone.
    assert_eq!(state.updated, 0);
    assert_eq!(state.writes.len(), 1);
    assert_eq!(state.user_updates.len(), 1);
}

#[tokio::test]
async fn transient_probe_failure_preserves_retry_after_and_retries() {
    let server = Server::with_scenario(Scenario::TransientProbe).await;
    sync_one(&server).await.unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.probes.len(), 2);
    assert!(state.probes[1].duration_since(state.probes[0]) >= Duration::from_secs(1));
    assert_eq!(state.writes.len(), 1);
    assert_eq!(state.user_updates.len(), 1);
}

#[tokio::test]
async fn stalled_folder_request_is_cancelled_at_total_folder_budget() {
    let server = Server::with_scenario(Scenario::StalledProbe).await;
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        sync_user_access(
            &server.client,
            "alice",
            &["test-stack-bucket-0"],
            "us-east-1",
            "test-access",
            "test-secret",
        ),
    )
    .await
    .expect("total folder budget should interrupt the stalled request");
    assert!(matches!(result, Err(Error::FolderTimeout { .. })));
    let state = server.state.lock().unwrap();
    assert_eq!(state.probes.len(), 1);
    assert!(state.writes.is_empty());
    assert!(state.user_updates.is_empty());
}
