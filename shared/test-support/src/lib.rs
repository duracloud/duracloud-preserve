use apputils::Stack;
use aws_sdk_s3::{Client, config::Region, primitives::SdkBody};
use aws_smithy_runtime::client::http::test_util::{ReplayEvent, StaticReplayClient};
use http::header::CONTENT_TYPE;

const DEFAULT_TEST_URI: &str = "https://test.s3.amazonaws.com/";

/// Construct a BucketCreator from shared integration test inputs.
#[macro_export]
macro_rules! bucket_creator {
    (
        $bucket:expr,
        $storage_tier_override:expr,
        $account_id:expr,
        $client:expr,
        $replication_role_arn:expr,
        $stack:expr $(,)?
    ) => {{
        awsutils::bucket_creator::BucketCreator::new(
            awsutils::bucket_creator::BucketCreatorParams {
                account_id: $account_id,
                client: $client,
                replication_role_arn: $replication_role_arn,
                stack: $stack,
            },
            $bucket,
            $storage_tier_override,
        )
    }};
}

/// Parse TEST_STACK from env (defaults to "int-test") for integration tests.
pub fn integration_test_stack() -> Stack {
    let stack_name = std::env::var("TEST_STACK").unwrap_or_else(|_| "int-test".to_string());
    Stack::new(&stack_name).unwrap_or_else(|e| panic!("invalid TEST_STACK '{}': {}", stack_name, e))
}

/// Build integration test config values from TEST_STACK.
pub async fn integration_test_config<T, E, F, Fut>(build: F) -> T
where
    F: FnOnce(Stack) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let stack = integration_test_stack();
    build(stack)
        .await
        .unwrap_or_else(|e| panic!("failed to build integration test config: {}", e))
}

/// Builder for creating test S3 clients with mock responses.
///
/// URIs are not matched by StaticReplayClient - responses are returned in order.
/// The URI parameter exists only for documentation/debugging purposes.
#[derive(Default)]
pub struct TestClientBuilder {
    events: Vec<ReplayEvent>,
}

impl TestClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the test client.
    pub fn build(self) -> Client {
        self.build_with_replay().0
    }

    /// Build the test client and return the replay handle for request assertions.
    pub fn build_with_replay(self) -> (Client, StaticReplayClient) {
        let http_client = StaticReplayClient::new(self.events);

        let config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .http_client(http_client.clone())
            .region(Region::new("us-east-1"))
            .build();

        (Client::from_conf(config), http_client)
    }

    /// Add an error response with XML error body.
    pub fn error(mut self, status: u16, error_code: &str, message: &str) -> Self {
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
        <Error>
            <Code>{}</Code>
            <Message>{}</Message>
        </Error>"#,
            error_code, message
        );

        self.events.push(ReplayEvent::new(
            http::Request::builder()
                .uri(DEFAULT_TEST_URI)
                .body(SdkBody::empty())
                .unwrap(),
            http::Response::builder()
                .status(status)
                .body(SdkBody::from(body))
                .unwrap(),
        ));
        self
    }

    /// Add a raw ReplayEvent.
    pub fn event(mut self, event: ReplayEvent) -> Self {
        self.events.push(event);
        self
    }

    /// Add an empty successful response.
    pub fn ok(self) -> Self {
        self.success(SdkBody::empty(), None)
    }

    /// Add a successful response with body.
    pub fn success(mut self, body: impl Into<SdkBody>, content_type: Option<String>) -> Self {
        let body = body.into();
        let mut response_builder = http::Response::builder().status(200);

        if let Some(ct) = content_type {
            response_builder = response_builder.header("Content-Type", ct);
        }

        if let Some(length) = body.content_length() {
            response_builder = response_builder.header("Content-Length", length.to_string());
        }

        self.events.push(ReplayEvent::new(
            http::Request::builder()
                .uri(DEFAULT_TEST_URI)
                .body(SdkBody::empty())
                .unwrap(),
            response_builder.body(body).unwrap(),
        ));
        self
    }

    /// Add an S3-specific error (BucketAlreadyExists, NoSuchBucket, etc.).
    pub fn s3_error(self, code: &str, message: &str) -> Self {
        let status = match code {
            "BucketAlreadyExists" | "BucketAlreadyOwnedByYou" => 409,
            "NoSuchBucket" | "NoSuchKey" => 404,
            "AccessDenied" => 403,
            _ => 400,
        };
        self.error(status, code, message)
    }
}

/// Minimal request record for test assertions.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub uri: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Convert replayed requests into plain data suitable for assertions.
///
/// `body` is request payload bytes only and does not include HTTP framing.
pub fn recorded_requests(replay: &StaticReplayClient) -> Vec<RecordedRequest> {
    replay
        .actual_requests()
        .map(|request| RecordedRequest {
            method: request.method().to_string(),
            uri: request.uri().to_string(),
            content_type: request.headers().get(CONTENT_TYPE).map(|v| v.to_string()),
            body: request.body().bytes().unwrap_or_default().to_vec(),
        })
        .collect()
}

/// Helper to create a ReplayEvent.
pub fn replay_event(uri: &str, status: u16, body: impl Into<SdkBody>) -> ReplayEvent {
    ReplayEvent::new(
        http::Request::builder()
            .uri(uri)
            .body(SdkBody::empty())
            .unwrap(),
        http::Response::builder()
            .status(status)
            .body(body.into())
            .unwrap(),
    )
}

/// Return current unix timestamp in seconds.
pub fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs()
}
