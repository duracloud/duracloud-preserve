use apputils::Stack;

use crate::bucket::RequestError;
use aws_config::{BehaviorVersion, SdkConfig};

/// AWS SDK clients
pub struct Clients {
    pub s3: aws_sdk_s3::Client,
    pub s3control: aws_sdk_s3control::Client,
}

impl Clients {
    pub fn new(sdk_config: &SdkConfig) -> Self {
        Self {
            s3: aws_sdk_s3::Client::new(sdk_config),
            s3control: aws_sdk_s3control::Client::new(sdk_config),
        }
    }

    /// Create Clients with a custom S3 client (for testing with mock clients)
    pub fn with_s3(sdk_config: &SdkConfig, s3: aws_sdk_s3::Client) -> Self {
        Self {
            s3,
            s3control: aws_sdk_s3control::Client::new(sdk_config),
        }
    }
}

/// Common configuration for all functions
pub struct Config {
    account_id: String,
    clients: Clients,
    pub debug_handler: bool,
    roles: Roles,
    sdk_config: SdkConfig,
    stack: Stack,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("account_id", &self.account_id)
            .field("debug_handler", &self.debug_handler)
            .field("roles", &self.roles)
            .field("stack", &self.stack)
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn new(
        sdk_config: SdkConfig,
        account_id: String,
        roles: Roles,
        stack: Stack,
        debug_handler: bool,
    ) -> Self {
        let clients = Clients::new(&sdk_config);
        Self {
            account_id,
            debug_handler,
            roles,
            stack,
            clients,
            sdk_config,
        }
    }

    /// Create a Config with pre-built clients (for testing with mock clients)
    pub fn new_with_clients(
        sdk_config: SdkConfig,
        account_id: String,
        roles: Roles,
        stack: Stack,
        debug_handler: bool,
        clients: Clients,
    ) -> Self {
        Self {
            account_id,
            debug_handler,
            roles,
            stack,
            clients,
            sdk_config,
        }
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn batch_role_arn(&self) -> &str {
        &self.roles.batch
    }

    pub fn replication_role_arn(&self) -> &str {
        &self.roles.replication
    }

    pub fn s3(&self) -> &aws_sdk_s3::Client {
        &self.clients.s3
    }

    pub fn s3control(&self) -> &aws_sdk_s3control::Client {
        &self.clients.s3control
    }

    pub fn sdk_config(&self) -> &SdkConfig {
        &self.sdk_config
    }

    pub fn stack(&self) -> &Stack {
        &self.stack
    }
}

/// Role ARNs for the stack
#[derive(Debug, Clone)]
pub struct Roles {
    pub batch: String,
    pub replication: String,
}

/// Create a Config for the stack
pub async fn config(stack: Stack) -> Config {
    let sdk_config = default_config().await;

    let account_id = get_account_id(&sdk_config).await.unwrap_or_else(|e| {
        eprintln!("get_account_id failed: {e:?}");
        panic!("failed to get account id");
    });

    let batch_role_name = stack.batch_role_name();
    let replication_role_name = stack.replication_role_name();

    let (batch_role, replication_role) = tokio::try_join!(
        get_role_arn(&sdk_config, &batch_role_name),
        get_role_arn(&sdk_config, &replication_role_name),
    )
    .unwrap_or_else(|e| {
        eprintln!("get_role_arn failed: {e:?}");
        panic!("failed to get role ARNs");
    });

    let roles = Roles {
        batch: batch_role,
        replication: replication_role,
    };

    Config::new(sdk_config, account_id, roles, stack, false)
}

/// Load default aws sdk config
pub async fn default_config() -> SdkConfig {
    aws_config::load_defaults(BehaviorVersion::latest()).await
}

/// Get the AWS account ID via STS
pub async fn get_account_id(config: &SdkConfig) -> Result<String, RequestError> {
    let sts_client = aws_sdk_sts::Client::new(config);
    let identity =
        sts_client.get_caller_identity().send().await.map_err(|e| {
            RequestError::ConfigError(format!("failed to get caller identity: {}", e))
        })?;

    identity
        .account()
        .map(|s| s.to_string())
        .ok_or_else(|| RequestError::ConfigError("no account ID in caller identity".to_string()))
}

/// Get the S3 Batch Operations role ARN for the stack.
pub async fn get_batch_role_arn(config: &SdkConfig, stack: &Stack) -> Result<String, RequestError> {
    get_role_arn(config, &stack.batch_role_name()).await
}

/// Extract the region from AWS S3 client configuration
pub fn get_region(client: &aws_sdk_s3::Client) -> Result<String, RequestError> {
    client
        .config()
        .region()
        .map(|r| r.to_string())
        .ok_or_else(|| RequestError::ConfigError("No region configured for S3 client".to_string()))
}

/// Get the S3 replication role ARN for the stack.
pub async fn get_replication_role_arn(
    config: &SdkConfig,
    stack: &Stack,
) -> Result<String, RequestError> {
    get_role_arn(config, &stack.replication_role_name()).await
}

/// Get an IAM role ARN by name.
/// Returns an error if the role does not exist.
async fn get_role_arn(config: &SdkConfig, role_name: &str) -> Result<String, RequestError> {
    let iam_client = aws_sdk_iam::Client::new(config);

    let response = iam_client
        .get_role()
        .role_name(role_name)
        .send()
        .await
        .map_err(|e| {
            RequestError::ConfigError(format!("failed to get role '{}': {}", role_name, e))
        })?;

    response
        .role()
        .map(|r| r.arn().to_string())
        .ok_or_else(|| RequestError::ConfigError("role missing ARN".to_string()))
}

/// Load test config from TEST_STACK env var (defaults to "int-test")
pub async fn test_config() -> Config {
    let stack_name = std::env::var("TEST_STACK").unwrap_or_else(|_| "int-test".to_string());
    let stack = Stack::new(&stack_name).expect("invalid stack name");
    config(stack).await
}
