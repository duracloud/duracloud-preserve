use crate::bucket::{Bucket, RequestConfig, RequestError, Type};
use crate::config::get_region;

/// Handles bucket setup by delegating to the appropriate methods per bucket type.
#[derive(Debug)]
pub struct BucketCreator<'a> {
    bucket: &'a Bucket,
    config: &'a RequestConfig,
}

impl<'a> BucketCreator<'a> {
    pub fn new(config: &'a RequestConfig, bucket: &'a Bucket) -> Self {
        Self { bucket, config }
    }

    pub async fn create(&self) -> Result<&Self, RequestError> {
        let region = get_region(&self.config.s3_client)?;
        let constraint = aws_sdk_s3::types::BucketLocationConstraint::from(region.as_str());

        let cfg = aws_sdk_s3::types::CreateBucketConfiguration::builder()
            .location_constraint(constraint)
            .build();

        println!("{:?}", cfg);

        // self.config
        //     .s3_client
        //     .create_bucket()
        //     .create_bucket_configuration(cfg)
        //     .bucket(self.bucket.0.as_str())
        //     .send()
        //     .await
        //     .map_err(|e| RequestError::S3Error(format!("failed to create bucket: {}", e)))?;

        Ok(self)
    }

    pub fn rollback(&self) -> Result<&Self, RequestError> {
        Ok(self)
    }

    pub fn setup(&self) -> Result<&Self, RequestError> {
        match self.bucket.1 {
            Type::Public => self.setup_public_bucket(),
            Type::Replication => self.setup_replication_bucket(),
            Type::Standard => self.setup_standard_bucket(),
        }
    }

    fn add_tags(&self) -> Result<&Self, RequestError> {
        Ok(self)
    }

    fn enable_inventory(&self) -> Result<&Self, RequestError> {
        Ok(self)
    }

    fn remove_deny_policy(&self) -> Result<&Self, RequestError> {
        Ok(self)
    }

    fn setup_public_bucket(&self) -> Result<&Self, RequestError> {
        self.add_tags()
            .and_then(|_| self.enable_inventory())
            .and_then(|_| self.remove_deny_policy())
    }

    fn setup_replication_bucket(&self) -> Result<&Self, RequestError> {
        self.add_tags()
    }

    fn setup_standard_bucket(&self) -> Result<&Self, RequestError> {
        self.add_tags()
            .and_then(|_| self.enable_inventory())
            .and_then(|_| self.remove_deny_policy())
    }
}
