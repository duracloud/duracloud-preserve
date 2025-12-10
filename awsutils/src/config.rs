use crate::bucket::RequestError;
use aws_config::{BehaviorVersion, SdkConfig};

/// Load default aws sdk config
pub async fn default_config() -> SdkConfig {
    aws_config::load_defaults(BehaviorVersion::latest()).await
}

/// Extract the region string from an AWS S3 client configuration
pub fn get_region(client: &aws_sdk_s3::Client) -> Result<String, RequestError> {
    client
        .config()
        .region()
        .map(|r| r.to_string())
        .ok_or_else(|| RequestError::S3Error("No region configured for S3 client".to_string()))
}
