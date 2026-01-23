use apputils::StackName;

use crate::bucket::RequestError;
use aws_config::{BehaviorVersion, SdkConfig};

/// Base configuration shared across request types
#[derive(Debug)]
pub struct BaseConfig {
    pub account_id: String,
    pub debug_handler: bool,
    pub role_arn: String,
    pub stack: StackName,
}

/// Configuration for S3 Batch/Control operations
#[derive(Debug)]
pub struct BatchConfig {
    pub base: BaseConfig,
    pub client: aws_sdk_s3control::Client,
}

impl BatchConfig {
    pub fn account_id(&self) -> &str {
        &self.base.account_id
    }
    pub fn role_arn(&self) -> &str {
        &self.base.role_arn
    }
    pub fn stack(&self) -> &StackName {
        &self.base.stack
    }
}

/// Configuration for S3 bucket operations
#[derive(Debug)]
pub struct RequestConfig {
    pub base: BaseConfig,
    pub client: aws_sdk_s3::Client,
}

impl RequestConfig {
    pub fn account_id(&self) -> &str {
        &self.base.account_id
    }
    pub fn role_arn(&self) -> &str {
        &self.base.role_arn
    }
    pub fn stack(&self) -> &StackName {
        &self.base.stack
    }
}

async fn base_config(sdk_config: &SdkConfig, stack: StackName, role_name: &str) -> BaseConfig {
    let account_id = get_account_id(sdk_config)
        .await
        .expect("failed to get account ID");
    let role_arn = get_role_arn(sdk_config, role_name)
        .await
        .expect("role not found");

    BaseConfig {
        account_id,
        debug_handler: false,
        role_arn,
        stack,
    }
}

pub async fn batch_config(stack: StackName) -> BatchConfig {
    let sdk_config = default_config().await;
    let role_name = stack.batch_role_name();
    let base = base_config(&sdk_config, stack, &role_name).await;
    let client = aws_sdk_s3control::Client::new(&sdk_config);

    BatchConfig { base, client }
}

/// Load default aws sdk config
pub async fn default_config() -> SdkConfig {
    aws_config::load_defaults(BehaviorVersion::latest()).await
}

/// Get the AWS account ID via STS
pub async fn get_account_id(config: &SdkConfig) -> Result<String, RequestError> {
    let sts_client = aws_sdk_sts::Client::new(config);
    let identity = sts_client
        .get_caller_identity()
        .send()
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to get caller identity: {}", e)))?;

    identity
        .account()
        .map(|s| s.to_string())
        .ok_or_else(|| RequestError::S3Error("no account ID in caller identity".to_string()))
}

/// Get the S3 Batch Operations role ARN for the stack.
pub async fn get_batch_role_arn(
    config: &SdkConfig,
    stack: &StackName,
) -> Result<String, RequestError> {
    get_role_arn(config, &stack.batch_role_name()).await
}

/// Extract the region string from an AWS S3 client configuration
pub fn get_region(client: &aws_sdk_s3::Client) -> Result<String, RequestError> {
    client
        .config()
        .region()
        .map(|r| r.to_string())
        .ok_or_else(|| RequestError::S3Error("No region configured for S3 client".to_string()))
}

/// Get the S3 replication role ARN for the stack.
pub async fn get_replication_role_arn(
    config: &SdkConfig,
    stack: &StackName,
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
        .map_err(|e| RequestError::S3Error(format!("failed to get role '{}': {}", role_name, e)))?;

    response
        .role()
        .map(|r| r.arn().to_string())
        .ok_or_else(|| RequestError::S3Error("role missing ARN".to_string()))
}

/// Load aws sdk config for a bucket request
pub async fn request_config(stack: StackName) -> RequestConfig {
    let sdk_config = default_config().await;
    let role_name = stack.replication_role_name();
    let base = base_config(&sdk_config, stack, &role_name).await;
    let client = aws_sdk_s3::Client::new(&sdk_config);

    RequestConfig { base, client }
}

/// Load test config from TEST_STACK env var (defaults to "inttest")
pub async fn test_config() -> RequestConfig {
    let stack_name = std::env::var("TEST_STACK").unwrap_or_else(|_| "inttest".to_string());
    let stack = StackName::new(&stack_name).expect("invalid stack name");
    request_config(stack).await
}
