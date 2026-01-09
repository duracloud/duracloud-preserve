use apputils::StackName;

use crate::bucket::{RequestConfig, RequestError};
use aws_config::{BehaviorVersion, SdkConfig};

/// Load aws sdk config for a bucket request
pub async fn bucket_config(stack: StackName) -> RequestConfig {
    let client_config = default_config().await;
    let s3_client = aws_sdk_s3::Client::new(&client_config);

    let account_id = get_account_id(&client_config)
        .await
        .expect("failed to get account ID");
    let replication_role_arn = get_replication_role_arn(&client_config, &stack)
        .await
        .expect("replication role not found - run scripts/create-replication-role.sh");

    RequestConfig {
        account_id,
        debug_handler: false,
        replication_role_arn,
        s3_client,
        stack,
    }
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

/// Extract the region string from an AWS S3 client configuration
pub fn get_region(client: &aws_sdk_s3::Client) -> Result<String, RequestError> {
    client
        .config()
        .region()
        .map(|r| r.to_string())
        .ok_or_else(|| RequestError::S3Error("No region configured for S3 client".to_string()))
}

/// Get the S3 replication role ARN for the stack.
/// Returns an error if the role does not exist.
pub async fn get_replication_role_arn(
    config: &SdkConfig,
    stack: &StackName,
) -> Result<String, RequestError> {
    let iam_client = aws_sdk_iam::Client::new(config);
    let role_name = stack.replication_role_name();

    let response = iam_client
        .get_role()
        .role_name(&role_name)
        .send()
        .await
        .map_err(|e| {
            RequestError::S3Error(format!(
                "failed to get replication role '{}': {}",
                role_name, e
            ))
        })?;

    response
        .role()
        .map(|r| r.arn().to_string())
        .ok_or_else(|| RequestError::S3Error("role missing ARN".to_string()))
}
