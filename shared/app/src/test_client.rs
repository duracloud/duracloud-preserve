use apputils::Stack;

use awsutils::test_client::TestClientBuilder;

use crate::config::{Clients, Config, Roles};

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

/// Create a Config for integration tests using TEST_STACK (defaults to "int-test").
pub async fn integration_test_config() -> Result<Config, awsutils::bucket::RequestError> {
    let stack_name = std::env::var("TEST_STACK").unwrap_or_else(|_| "int-test".to_string());
    let stack = Stack::new(&stack_name).map_err(|e| {
        awsutils::bucket::RequestError::ConfigError(format!(
            "invalid stack name '{}': {}",
            stack_name, e
        ))
    })?;
    crate::config::config(stack).await
}
