use crate::bucket::RequestError;
use aws_config::{BehaviorVersion, SdkConfig};

/// Load default aws sdk config.
pub async fn default_config() -> SdkConfig {
    aws_config::load_defaults(BehaviorVersion::latest()).await
}

/// Get the AWS account ID via STS.
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

/// Extract the region from AWS S3 client configuration.
pub fn get_region(client: &aws_sdk_s3::Client) -> Result<String, RequestError> {
    client
        .config()
        .region()
        .map(|r| r.to_string())
        .ok_or_else(|| RequestError::ConfigError("No region configured for S3 client".to_string()))
}

/// Get an IAM role ARN by name.
/// Returns an error if the role does not exist.
pub async fn get_role_arn(config: &SdkConfig, role_name: &str) -> Result<String, RequestError> {
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
