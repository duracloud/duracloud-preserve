use apputils::content_type::TEXT_PLAIN;
use aws_sdk_s3::types::TransitionStorageClass;
use aws_smithy_types::body::SdkBody;

use awsutils::{
    bucket_creator,
    file::{self, File},
};

use crate::{bucket, config::Config, perform::errors::BucketRequestError};

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

/// Process a bucket creation request file from S3.
pub async fn perform(
    config: &Config,
    file: &File,
    opts: &PerformOptions,
) -> Result<(), BucketRequestError> {
    tracing::info!("Retrieving request file from S3: {}", file.s3_url());

    let names = match awsutils::bucket::get_bucket_names(config.s3(), file).await {
        Ok(names) => names,
        Err(e) => {
            tracing::error!("Error getting bucket names: {}", e);
            if let Err(fb_err) = file::feedback(
                config.s3(),
                config.stack(),
                file.key(),
                SdkBody::from(e.to_string()),
                TEXT_PLAIN,
            )
            .await
            {
                tracing::error!("Failed to upload feedback: {fb_err}");
            }
            return Err(BucketRequestError::RequestFile(e));
        }
    };

    tracing::info!("Bucket names: {:?}", names);
    tracing::info!("Parsing bucket names");

    let buckets = match apputils::bucket::review_bucket_names(config.stack(), &names) {
        Ok(buckets) => buckets,
        Err(e) => {
            tracing::error!("Error parsing bucket names: {}", e);
            if let Err(fb_err) = file::feedback(
                config.s3(),
                config.stack(),
                file.key(),
                SdkBody::from(e.to_string()),
                TEXT_PLAIN,
            )
            .await
            {
                tracing::error!("Failed to upload feedback: {fb_err}");
            }
            return Err(BucketRequestError::Validation(e));
        }
    };

    tracing::info!("Buckets to create: {:?}", buckets);
    tracing::info!("Creating buckets");

    let issues = bucket::create_buckets(config, &buckets, opts.standard_storage_tier.clone()).await;
    if !issues.is_empty() {
        tracing::error!("{:?}", issues);
        if let Err(fb_err) = file::feedback(
            config.s3(),
            config.stack(),
            file.key(),
            SdkBody::from(issues.join("\n")),
            TEXT_PLAIN,
        )
        .await
        {
            tracing::error!("Failed to upload feedback: {fb_err}");
        }
        return Err(BucketRequestError::CreateBuckets(issues));
    }

    tracing::info!("Perform complete");
    file::delete(config.s3(), file)
        .await
        .map_err(BucketRequestError::Cleanup)?;
    Ok(())
}
