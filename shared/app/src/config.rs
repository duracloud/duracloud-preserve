use apputils::Stack;
use aws_config::SdkConfig;

use awsutils::{bucket::RequestError, config as aws_config_utils};

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

/// Create a Config for the stack.
pub async fn config(stack: Stack) -> Result<Config, RequestError> {
    let sdk_config = aws_config_utils::default_config().await;

    let account_id = aws_config_utils::get_account_id(&sdk_config).await?;

    let (batch_role, replication_role) = tokio::try_join!(
        get_batch_role_arn(&sdk_config, &stack),
        get_replication_role_arn(&sdk_config, &stack),
    )?;

    let roles = Roles {
        batch: batch_role,
        replication: replication_role,
    };

    Ok(Config::new(sdk_config, account_id, roles, stack, false))
}

/// Get the S3 Batch Operations role ARN for the stack.
pub async fn get_batch_role_arn(config: &SdkConfig, stack: &Stack) -> Result<String, RequestError> {
    aws_config_utils::get_role_arn(config, &stack.batch_role_name()).await
}

/// Get the S3 replication role ARN for the stack.
pub async fn get_replication_role_arn(
    config: &SdkConfig,
    stack: &Stack,
) -> Result<String, RequestError> {
    aws_config_utils::get_role_arn(config, &stack.replication_role_name()).await
}
