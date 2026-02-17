use apputils::Stack;
use aws_sdk_s3::{Client, types::TransitionStorageClass};

use awsutils::{
    bucket::{self, BUCKET_TAG_STACK_KEY, Bucket, RequestError, Type},
    bucket_creator::{BucketCreator, BucketCreatorParams},
};

use crate::config::Config;

/// Create and setup an S3 bucket. If setup fails, attempt rollback.
/// Returns the BucketCreator for follow-up operations (e.g., enable_replication).
async fn create<'a>(
    config: &'a Config,
    bucket: &'a Bucket,
    storage_tier_override: Option<TransitionStorageClass>,
) -> Result<BucketCreator<'a>, RequestError> {
    let creator = BucketCreator::new(
        BucketCreatorParams {
            account_id: config.account_id(),
            client: config.s3(),
            replication_role_arn: config.replication_role_arn(),
            stack: config.stack(),
        },
        bucket,
        storage_tier_override,
    );

    creator.create().await?;

    if let Err(e) = creator.setup().await {
        if let Err(rollback_err) = creator.rollback().await {
            tracing::error!("Rollback failed: {}", rollback_err);
        }
        return Err(e);
    }

    Ok(creator)
}

/// Create primary and replication buckets.
pub async fn create_buckets(
    config: &Config,
    buckets: &[(Bucket, Bucket)],
    standard_storage_tier: TransitionStorageClass,
) -> Vec<String> {
    let mut issues = Vec::new();

    for (primary, replication) in buckets {
        // For now we only allow storage-tier override on standard buckets.
        let primary_storage_tier_override = match primary.bucket_type() {
            Type::Standard => Some(standard_storage_tier.clone()),
            _ => None,
        };

        let replication_storage_tier_override = None;

        let result = create_bucket_pair(
            config,
            primary,
            replication,
            primary_storage_tier_override,
            replication_storage_tier_override,
        )
        .await;

        if let Err(e) = &result {
            issues.push(e.to_string());
        }
    }

    issues
}

/// Create a primary bucket and its replication bucket, then enable replication.
pub async fn create_bucket_pair(
    config: &Config,
    primary: &Bucket,
    replication: &Bucket,
    primary_storage_tier_override: Option<TransitionStorageClass>,
    replication_storage_tier_override: Option<TransitionStorageClass>,
) -> Result<(), RequestError> {
    let primary_creator = create(config, primary, primary_storage_tier_override).await?;

    let repl_creator = match create(config, replication, replication_storage_tier_override).await {
        Ok(creator) => creator,
        Err(e) => {
            if let Err(rollback_err) = primary_creator.rollback().await {
                tracing::error!("Rollback of {} failed: {}", primary.name(), rollback_err);
            }
            return Err(e);
        }
    };

    if let Err(e) = primary_creator.enable_replication(replication).await {
        if let Err(rollback_err) = primary_creator.rollback().await {
            tracing::error!("Rollback of {} failed: {}", primary.name(), rollback_err);
        }
        if let Err(rollback_err) = repl_creator.rollback().await {
            tracing::error!(
                "Rollback of {} failed: {}",
                replication.name(),
                rollback_err
            );
        }
        return Err(e);
    }

    Ok(())
}

/// Get all buckets belonging to a stack (prefix match + stack tag).
pub async fn get_stack_buckets(
    client: &Client,
    stack: &Stack,
) -> Result<Vec<Bucket>, RequestError> {
    let prefix = format!("{}-", stack.as_str());
    let mut buckets = Vec::new();

    let response = client
        .list_buckets()
        .send()
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to list buckets: {}", e)))?;

    for bucket in response.buckets() {
        let Some(name) = bucket.name() else {
            continue;
        };

        if !name.starts_with(&prefix) {
            continue;
        }

        let Ok(tag_response) = client.get_bucket_tagging().bucket(name).send().await else {
            continue;
        };

        let tags = tag_response.tag_set();
        let stack_matches = tags
            .iter()
            .any(|tag| tag.key() == BUCKET_TAG_STACK_KEY && tag.value() == stack.as_str());

        if !stack_matches {
            continue;
        }

        if let Some(bucket) = bucket::from_tags(name, tags)? {
            buckets.push(bucket);
        }
    }

    Ok(buckets)
}

/// Get stack buckets filtered by type.
pub async fn get_stack_buckets_by_type(
    client: &Client,
    stack: &Stack,
    types: &[Type],
) -> Result<Vec<Bucket>, RequestError> {
    let all_buckets = get_stack_buckets(client, stack).await?;
    Ok(all_buckets
        .into_iter()
        .filter(|b| types.contains(b.bucket_type()))
        .collect())
}
