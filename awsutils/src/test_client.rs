use aws_sdk_s3::{Client, config::Region, primitives::SdkBody};
use aws_smithy_runtime::client::http::test_util::{ReplayEvent, StaticReplayClient};
// https://github.com/awsdocs/aws-doc-sdk-examples/blob/main/rustv1/examples/s3/src/lib.rs#L192

const DEFAULT_TEST_URI: &str = "https://test.s3.amazonaws.com/";

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

    /// Add a successful response with body
    pub fn success(mut self, body: impl Into<SdkBody>) -> Self {
        let body = body.into();
        let mut response_builder = http::Response::builder().status(200);

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

    /// Add an empty successful response
    pub fn ok(self) -> Self {
        self.success(SdkBody::empty())
    }

    /// Add multiple empty successful responses (unused events are ignored)
    pub fn ok_n(mut self, n: usize) -> Self {
        for _ in 0..n {
            self = self.ok();
        }
        self
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

    /// Build the test client
    pub fn build(self) -> Client {
        let http_client = StaticReplayClient::new(self.events);

        let config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .http_client(http_client)
            .region(Region::new("us-east-1"))
            .build();

        Client::from_conf(config)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_success() {
        let _client = TestClientBuilder::new().success("content").build();
    }

    #[test]
    fn test_builder_ok() {
        let _client = TestClientBuilder::new().ok().build();
    }

    #[test]
    fn test_builder_ok_n() {
        let _client = TestClientBuilder::new().ok_n(50).build();
    }

    #[test]
    fn test_builder_error() {
        let _client = TestClientBuilder::new()
            .error(404, "NoSuchKey", "The specified key does not exist.")
            .build();
    }

    #[test]
    fn test_builder_multi() {
        let _client = TestClientBuilder::new()
            .ok()
            .success("data")
            .error(500, "InternalError", "Internal server error")
            .build();
    }
}
