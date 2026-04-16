use aws_config::SdkConfig;
use base::Stack;

use awsutils::{
    bucket::{self, RequestError},
    config as aws_config_utils,
};

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
}

/// Common configuration for all functions
pub struct Config {
    account_id: String,
    clients: Clients,
    pub debug_handler: bool,
    owner: String,
    roles: Roles,
    sdk_config: SdkConfig,
    stack: Stack,
    storage_capacity: u64,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("account_id", &self.account_id)
            .field("debug_handler", &self.debug_handler)
            .field("owner", &self.owner)
            .field("roles", &self.roles)
            .field("stack", &self.stack)
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn new(
        sdk_config: SdkConfig,
        account_id: String,
        owner: String,
        roles: Roles,
        stack: Stack,
        storage_capacity: u64,
        debug_handler: bool,
    ) -> Self {
        let clients = Clients::new(&sdk_config);
        Self {
            account_id,
            debug_handler,
            owner,
            roles,
            stack,
            clients,
            sdk_config,
            storage_capacity,
        }
    }

    /// Create a Config for tests from a mocked SDK config.
    pub fn for_tests(sdk_config: SdkConfig, debug_handler: bool) -> Self {
        let roles = Roles {
            batch: "arn:aws:iam::123456789:role/test-batch-role".to_string(),
            replication: "arn:aws:iam::123456789:role/test-replication-role".to_string(),
        };
        let stack = Stack::new("test-stack").expect("test stack should be valid");
        let clients = Clients::new(&sdk_config);

        Self {
            account_id: "123456789".to_string(),
            debug_handler,
            owner: "Test Owner".to_string(),
            roles,
            stack,
            clients,
            sdk_config,
            storage_capacity: 0,
        }
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn batch_role_arn(&self) -> &str {
        &self.roles.batch
    }

    pub fn owner(&self) -> &str {
        &self.owner
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

    pub fn storage_capacity(&self) -> u64 {
        self.storage_capacity
    }
}

/// Role ARNs for the stack.
#[derive(Debug, Clone)]
pub struct Roles {
    pub batch: String,
    pub replication: String,
}

/// Create a Config for the stack.
pub async fn load(stack: Stack) -> Result<Config, RequestError> {
    let sdk_config = aws_config_utils::load_defaults().await;
    let managed_bucket = stack.managed_bucket();

    let account = aws_sdk_account::Client::new(&sdk_config);
    let iam = aws_sdk_iam::Client::new(&sdk_config);
    let s3 = aws_sdk_s3::Client::new(&sdk_config);
    let ssm = aws_sdk_ssm::Client::new(&sdk_config);
    let sts = aws_sdk_sts::Client::new(&sdk_config);

    if !bucket::exists(&s3, &managed_bucket).await {
        return Err(RequestError::ConfigError(format!(
            "failed to find managed bucket for stack (does this stack exist?): {}",
            &managed_bucket
        )));
    }

    let account_id = aws_config_utils::get_account_id(&sts).await?;
    let batch_role_name = stack.batch_role_name();
    let storage_capacity_param_name = stack.storage_capacity_param_name();
    let replication_role_name = stack.replication_role_name();

    let (batch_role, replication_role, owner, storage_capacity) = tokio::try_join!(
        aws_config_utils::get_role_arn(&iam, &batch_role_name),
        aws_config_utils::get_role_arn(&iam, &replication_role_name),
        aws_config_utils::get_account_name(&account),
        aws_config_utils::get_parameter(&ssm, &storage_capacity_param_name),
    )?;

    let roles = Roles {
        batch: batch_role,
        replication: replication_role,
    };

    let storage_capacity = storage_capacity.parse::<u64>().map_err(|e| {
        RequestError::ValidationError(format!("failed to parse storage capacity: {}", e))
    })?;

    Ok(Config::new(
        sdk_config,
        account_id,
        owner,
        roles,
        stack,
        storage_capacity,
        false,
    ))
}
