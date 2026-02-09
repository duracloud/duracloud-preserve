use apputils::Stack;
use aws_sdk_s3::{Client, config::Region, primitives::SdkBody};
use aws_smithy_runtime::client::http::test_util::{ReplayEvent, StaticReplayClient};
use http::header::CONTENT_TYPE;

use crate::config::{Clients, Config, Roles};
// https://github.com/awsdocs/aws-doc-sdk-examples/blob/main/rustv1/examples/s3/src/lib.rs#L192

const DEFAULT_TEST_URI: &str = "https://test.s3.amazonaws.com/";

/// Builder for creating mock Config values in tests.
pub struct MockConfigBuilder {
    client: aws_sdk_s3::Client,
    stack: Stack,
    debug_handler: bool,
}

impl Default for MockConfigBuilder {
    fn default() -> Self {
        Self {
            client: TestClientBuilder::new().ok().build(),
            stack: Stack::new("test-stack").unwrap(),
            debug_handler: false,
        }
    }
}

impl MockConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn client(mut self, client: aws_sdk_s3::Client) -> Self {
        self.client = client;
        self
    }

    pub fn stack(mut self, stack: Stack) -> Self {
        self.stack = stack;
        self
    }

    pub fn debug_handler(mut self, debug_handler: bool) -> Self {
        self.debug_handler = debug_handler;
        self
    }

    pub fn build(self) -> Config {
        build_mock_config(self.client, self.stack, self.debug_handler)
    }
}

fn build_mock_config(client: aws_sdk_s3::Client, stack: Stack, debug_handler: bool) -> Config {
    let sdk_config = aws_config::SdkConfig::builder()
        .behavior_version(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new("us-east-1"))
        .build();

    let roles = Roles {
        batch: "arn:aws:iam::123456789:role/test-batch-role".to_string(),
        replication: "arn:aws:iam::123456789:role/test-replication-role".to_string(),
    };

    let clients = Clients::with_s3(&sdk_config, client);

    Config::new_with_clients(
        sdk_config,
        "123456789".to_string(),
        roles,
        stack,
        debug_handler,
        clients,
    )
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

    /// Build the test client
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

    /// Add an error response with XML error body
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

    /// Add a raw ReplayEvent
    pub fn event(mut self, event: ReplayEvent) -> Self {
        self.events.push(event);
        self
    }

    /// Add an empty successful response
    pub fn ok(self) -> Self {
        self.success(SdkBody::empty(), None)
    }

    /// Add a successful response with body
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

    /// Add an S3-specific error (BucketAlreadyExists, NoSuchBucket, etc.)
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

/// Create a Config for integration tests using TEST_STACK (defaults to "int-test").
pub async fn integration_test_config() -> Config {
    let stack_name = std::env::var("TEST_STACK").unwrap_or_else(|_| "int-test".to_string());
    let stack = Stack::new(&stack_name).expect("invalid stack name");
    crate::config::config(stack).await
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

/// Helper to create a ReplayEvent
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
