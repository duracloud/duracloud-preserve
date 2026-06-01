use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_s3::{Client, primitives::SdkBody};
use aws_smithy_http_client::test_util::{ReplayEvent, StaticReplayClient};
use base::Stack;
use constants::TEXT_XML;
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
        let (sdk_config, replay) = self.build_sdk_config_with_replay();
        let config = aws_sdk_s3::Config::from(&sdk_config);

        (Client::from_conf(config), replay)
    }

    /// Build a mocked SDK config for constructing any AWS service client in tests.
    pub fn build_sdk_config(self) -> SdkConfig {
        self.build_sdk_config_with_replay().0
    }

    /// Build a mocked SDK config and return the replay handle for request assertions.
    pub fn build_sdk_config_with_replay(self) -> (SdkConfig, StaticReplayClient) {
        mock_sdk_config_with_events(self.events)
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

    /// Add a successful response with body and custom headers.
    pub fn success_with_headers(
        mut self,
        body: impl Into<SdkBody>,
        headers: &[(&str, &str)],
    ) -> Self {
        let body = body.into();
        let mut response_builder = http::Response::builder().status(200);

        for (name, value) in headers {
            response_builder = response_builder.header(*name, *value);
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

/// Build an AWS SDK config and replay handle for service clients under test.
pub fn mock_sdk_config(event: ReplayEvent) -> (SdkConfig, StaticReplayClient) {
    mock_sdk_config_with_events(vec![event])
}

/// Build an AWS SDK config and replay handle for service clients under test with multiple events.
pub fn mock_sdk_config_with_events(events: Vec<ReplayEvent>) -> (SdkConfig, StaticReplayClient) {
    let http_client = StaticReplayClient::new(events);
    let sdk_config = SdkConfig::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_sdk_s3::config::SharedCredentialsProvider::new(
            aws_sdk_s3::config::Credentials::for_tests(),
        ))
        .http_client(http_client.clone())
        .build();

    (sdk_config, http_client)
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

/// Helper to create a ReplayEvent with an optional content type response header.
pub fn replay_event_with_content_type(
    uri: &str,
    status: u16,
    body: impl Into<SdkBody>,
    content_type: Option<&str>,
) -> ReplayEvent {
    let mut response = http::Response::builder().status(status);
    if let Some(content_type) = content_type {
        response = response.header("Content-Type", content_type);
    }

    ReplayEvent::new(
        http::Request::builder()
            .uri(uri)
            .body(SdkBody::empty())
            .unwrap(),
        response.body(body.into()).unwrap(),
    )
}

/// Helper to create an XML ReplayEvent using the default test URI.
pub fn replay_xml_event(status: u16, body: impl Into<String>) -> ReplayEvent {
    replay_event_with_content_type(
        DEFAULT_TEST_URI,
        status,
        SdkBody::from(body.into()),
        Some(TEXT_XML),
    )
}

/// Build an s3control `DescribeJob` XML response body for tests.
///
/// Pass `failure_code` to include a single `FailureReasons` entry -- e.g.
/// `Some("InvalidManifestContent")` to simulate a job that failed only because
/// its generated manifest matched no objects.
pub fn describe_job_xml(job_id: &str, status: &str, failure_code: Option<&str>) -> String {
    let failure_reasons = failure_code
        .map(|code| {
            format!(
                "<FailureReasons><member><FailureCode>{code}</FailureCode>\
                 <FailureReason>test failure</FailureReason></member></FailureReasons>"
            )
        })
        .unwrap_or_default();

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><DescribeJobResult><Job><JobId>{job_id}</JobId><Status>{status}</Status>{failure_reasons}</Job></DescribeJobResult>"#
    )
}

/// Return current unix timestamp in seconds.
pub fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs()
}
