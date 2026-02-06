use crate::bucket::{self, RequestError};
use crate::config::Config;
use crate::file::{self, File};
use tracing;

/// Process a bucket creation request file from S3
pub async fn perform(config: &Config, file: &File) -> Result<(), RequestError> {
    tracing::info!("Retrieving request file from S3: {}", file.s3_url());

    let names = match bucket::get_bucket_names(config.s3(), file).await {
        Ok(names) => names,
        Err(e) => {
            tracing::error!("Error getting bucket names: {}", e);
            // TODO: upload error report
            return Err(e);
        }
    };

    tracing::info!("Bucket names: {:?}", names);
    tracing::info!("Parsing bucket names");

    let buckets = match bucket::review_bucket_names(config, &names) {
        Ok(buckets) => buckets,
        Err(e) => {
            tracing::error!("Error parsing bucket names: {}", e);
            // TODO: upload error report
            return Err(e);
        }
    };

    tracing::info!("Buckets to create: {:?}", buckets);
    tracing::info!("Creating buckets");

    let issues = bucket::create_buckets(config, &buckets).await;
    if !issues.is_empty() {
        // TODO: upload the issues
        tracing::error!("{:?}", issues);
        return Err(RequestError::S3Error(format!(
            "Failed to create one or more buckets: {}",
            issues.join("; ")
        )));
    }

    tracing::info!("Perform complete");
    file::delete(config.s3(), file).await?;
    Ok(())
}
