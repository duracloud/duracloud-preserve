use aws_sdk_s3::types::TransitionStorageClass;
use awsutils::{
    bucket_creator,
    file::{self, File},
};
use constants::{SYNC_USERS_FILE, TEXT_PLAIN};

use crate::{bucket, config::Config, errors::BucketRequestError, upload};

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub request_file: File,
    pub standard_storage_tier: TransitionStorageClass,
    pub trigger_sync_users: bool,
}

impl PerformArgs {
    pub fn new(request_file: File) -> Self {
        Self {
            request_file,
            standard_storage_tier: bucket_creator::STORAGE_CLASS_STANDARD_DEFAULT,
            trigger_sync_users: false,
        }
    }
}

/// Process a bucket creation request file from S3.
pub async fn perform(config: &Config, args: &PerformArgs) -> Result<(), BucketRequestError> {
    let file = &args.request_file;
    tracing::info!("Retrieving request file from S3: {}", file.s3_url());

    let names = match bucket::read_request_names(config.s3(), file).await {
        Ok(names) => names,
        Err(e) => {
            tracing::error!("Error getting bucket names: {}", e);
            upload::put_feedback(config, file.key(), e.to_string()).await;
            return Err(BucketRequestError::RequestFile(e));
        }
    };

    tracing::info!("Bucket names: {:?}", names);
    tracing::info!("Parsing bucket names");

    let buckets = match bucket::review_request_names(config.stack(), &names) {
        Ok(buckets) => buckets,
        Err(e) => {
            tracing::error!("Error parsing bucket names: {}", e);
            upload::put_feedback(config, file.key(), e.to_string()).await;
            return Err(BucketRequestError::Validation(e));
        }
    };

    tracing::info!("Buckets to create: {:?}", buckets);
    tracing::info!("Creating buckets");

    let issues = bucket::create_pairs(config, &buckets, args.standard_storage_tier.clone()).await;
    if !issues.is_empty() {
        tracing::error!("{:?}", issues);
        upload::put_feedback(config, file.key(), issues.join("\n")).await;
        return Err(BucketRequestError::CreateBuckets(issues));
    }

    tracing::info!("Perform complete");
    file::delete(config.s3(), file)
        .await
        .map_err(BucketRequestError::Cleanup)?;

    if args.trigger_sync_users {
        let trigger = File::from(config.stack().sync_users_path(SYNC_USERS_FILE));
        tracing::info!("Uploading sync-users trigger: {}", trigger.s3_url());
        file::upload(config.s3(), &trigger, Vec::new(), TEXT_PLAIN)
            .await
            .map_err(BucketRequestError::TriggerSyncUsers)?;
    }

    Ok(())
}
