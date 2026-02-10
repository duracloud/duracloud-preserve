use aws_sdk_s3::types::TransitionStorageClass;
use aws_smithy_types::body::SdkBody;
use tracing;

use apputils::content_type::TEXT_PLAIN;

use crate::bucket::{self, RequestError};
use crate::bucket_creator;
use crate::config::Config;
use crate::file::{self, File};

#[derive(Debug, Clone)]
pub struct PerformOptions {
    pub standard_storage_tier: TransitionStorageClass,
}

impl Default for PerformOptions {
    fn default() -> Self {
        Self {
            standard_storage_tier: bucket_creator::STORAGE_CLASS_STANDARD_DEFAULT,
        }
    }
}

/// Process a bucket creation request file from S3
pub async fn perform(
    config: &Config,
    file: &File,
    opts: &PerformOptions,
) -> Result<(), RequestError> {
    tracing::info!("Retrieving request file from S3: {}", file.s3_url());

    let names = match bucket::get_bucket_names(config.s3(), file).await {
        Ok(names) => names,
        Err(e) => {
            tracing::error!("Error getting bucket names: {}", e);
            file::feedback(config, file.key(), SdkBody::from(e.to_string()), TEXT_PLAIN).await?;
            return Err(e);
        }
    };

    tracing::info!("Bucket names: {:?}", names);
    tracing::info!("Parsing bucket names");

    let buckets = match bucket::review_bucket_names(config, &names) {
        Ok(buckets) => buckets,
        Err(e) => {
            tracing::error!("Error parsing bucket names: {}", e);
            file::feedback(config, file.key(), SdkBody::from(e.to_string()), TEXT_PLAIN).await?;
            return Err(e);
        }
    };

    tracing::info!("Buckets to create: {:?}", buckets);
    tracing::info!("Creating buckets");

    let issues = bucket::create_buckets(config, &buckets, opts.standard_storage_tier.clone()).await;
    if !issues.is_empty() {
        tracing::error!("{:?}", issues);
        file::feedback(
            config,
            file.key(),
            SdkBody::from(issues.join("\n")),
            TEXT_PLAIN,
        )
        .await?;
        return Err(RequestError::S3Error(format!(
            "Failed to create one or more buckets: {}",
            issues.join("; ")
        )));
    }

    tracing::info!("Perform complete");
    file::delete(config.s3(), file).await?;
    Ok(())
}
