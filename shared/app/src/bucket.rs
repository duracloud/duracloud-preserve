use apputils::Stack;
use aws_sdk_s3::{Client, types::TransitionStorageClass};

use awsutils::{
    bucket::{self, BUCKET_TAG_STACK_KEY, Bucket, BucketPair, RequestError, Type},
    bucket_creator::{
        BUCKET_TAG_ORIGIN_KEY, BUCKET_TAG_ORIGIN_VAL, BucketCreator, BucketCreatorParams,
    },
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
    buckets: &[BucketPair],
    standard_storage_tier: TransitionStorageClass,
) -> Vec<String> {
    let mut issues = Vec::new();

    for BucketPair {
        source: primary,
        replication,
    } in buckets
    {
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

/// Get stack buckets that were created via bucket-request (BucketOrigin=bucket-request).
/// These are the buckets eligible for reconciliation.
pub async fn get_bucket_request_buckets(
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

    for b in response.buckets() {
        let Some(name) = b.name() else {
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

        let origin_matches = tags
            .iter()
            .any(|tag| tag.key() == BUCKET_TAG_ORIGIN_KEY && tag.value() == BUCKET_TAG_ORIGIN_VAL);

        if !origin_matches {
            continue;
        }

        if let Some(bucket) = bucket::from_tags(name, tags)? {
            buckets.push(bucket);
        }
    }

    Ok(buckets)
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

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::TestClientBuilder;

    fn list_buckets_xml(names: &[&str]) -> String {
        let buckets = names
            .iter()
            .map(|name| {
                format!(
                    "<Bucket><Name>{name}</Name><CreationDate>2025-01-01T00:00:00.000Z</CreationDate></Bucket>"
                )
            })
            .collect::<Vec<_>>()
            .join("");

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Owner>
    <ID>owner-id</ID>
    <DisplayName>owner</DisplayName>
  </Owner>
  <Buckets>{buckets}</Buckets>
</ListAllMyBucketsResult>"#
        )
    }

    fn bucket_tagging_xml(tags: &[(&str, &str)]) -> String {
        let entries = tags
            .iter()
            .map(|(k, v)| format!("<Tag><Key>{k}</Key><Value>{v}</Value></Tag>"))
            .collect::<Vec<_>>()
            .join("");

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet>{entries}</TagSet>
</Tagging>"#
        )
    }

    #[tokio::test]
    async fn test_get_stack_buckets_skips_tag_lookup_failures() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&["test-stack-alpha", "test-stack-bravo"]);
        let bravo_tags = bucket_tagging_xml(&[("Stack", "test-stack"), ("BucketType", "standard")]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .s3_error("AccessDenied", "tagging denied")
            .success(bravo_tags, None)
            .build();

        let buckets = get_stack_buckets(&client, &stack).await.unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name(), "test-stack-bravo");
        assert_eq!(buckets[0].bucket_type(), &Type::Standard);
    }

    #[tokio::test]
    async fn test_get_stack_buckets_excludes_non_matching_stack_tag() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&["test-stack-alpha"]);
        let tags = bucket_tagging_xml(&[("Stack", "other-stack"), ("BucketType", "standard")]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .success(tags, None)
            .build();

        let buckets = get_stack_buckets(&client, &stack).await.unwrap();
        assert!(buckets.is_empty());
    }

    #[tokio::test]
    async fn test_get_stack_buckets_excludes_missing_or_invalid_bucket_type() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&["test-stack-alpha", "test-stack-bravo"]);
        let missing_type_tags = bucket_tagging_xml(&[("Stack", "test-stack")]);
        let invalid_type_tags = bucket_tagging_xml(&[
            ("Stack", "test-stack"),
            ("BucketType", "not-a-supported-type"),
        ]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .success(missing_type_tags, None)
            .success(invalid_type_tags, None)
            .build();

        let buckets = get_stack_buckets(&client, &stack).await.unwrap();
        assert!(buckets.is_empty());
    }

    #[tokio::test]
    async fn test_get_stack_buckets_by_type_filters_results() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&["test-stack-public", "test-stack-repl"]);
        let public_tags = bucket_tagging_xml(&[("Stack", "test-stack"), ("BucketType", "public")]);
        let repl_tags =
            bucket_tagging_xml(&[("Stack", "test-stack"), ("BucketType", "replication")]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .success(public_tags, None)
            .success(repl_tags, None)
            .build();

        let buckets = get_stack_buckets_by_type(&client, &stack, &[Type::Public])
            .await
            .unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name(), "test-stack-public");
        assert_eq!(buckets[0].bucket_type(), &Type::Public);
    }

    #[tokio::test]
    async fn test_get_bucket_request_buckets_filters_by_origin_tag() {
        let stack = Stack::new("test-stack").unwrap();
        let list =
            list_buckets_xml(&["test-stack-alpha", "test-stack-bravo", "test-stack-managed"]);

        // alpha: bucket-request origin (included)
        let alpha_tags = bucket_tagging_xml(&[
            ("Stack", "test-stack"),
            ("BucketType", "standard"),
            ("BucketOrigin", "bucket-request"),
        ]);
        // bravo: terraform origin (excluded)
        let bravo_tags = bucket_tagging_xml(&[
            ("Stack", "test-stack"),
            ("BucketType", "standard"),
            ("BucketOrigin", "terraform"),
        ]);
        // managed: no origin tag (excluded)
        let managed_tags =
            bucket_tagging_xml(&[("Stack", "test-stack"), ("BucketType", "internal")]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .success(alpha_tags, None)
            .success(bravo_tags, None)
            .success(managed_tags, None)
            .build();

        let buckets = get_bucket_request_buckets(&client, &stack).await.unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name(), "test-stack-alpha");
        assert_eq!(buckets[0].bucket_type(), &Type::Standard);
    }

    #[tokio::test]
    async fn test_get_bucket_request_buckets_includes_multiple_types() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&[
            "test-stack-data",
            "test-stack-data-public",
            "test-stack-data-repl",
        ]);

        let data_tags = bucket_tagging_xml(&[
            ("Stack", "test-stack"),
            ("BucketType", "standard"),
            ("BucketOrigin", "bucket-request"),
        ]);
        let public_tags = bucket_tagging_xml(&[
            ("Stack", "test-stack"),
            ("BucketType", "public"),
            ("BucketOrigin", "bucket-request"),
        ]);
        let repl_tags = bucket_tagging_xml(&[
            ("Stack", "test-stack"),
            ("BucketType", "replication"),
            ("BucketOrigin", "bucket-request"),
        ]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .success(data_tags, None)
            .success(public_tags, None)
            .success(repl_tags, None)
            .build();

        let buckets = get_bucket_request_buckets(&client, &stack).await.unwrap();

        assert_eq!(buckets.len(), 3);
        let types: Vec<&Type> = buckets.iter().map(|b| b.bucket_type()).collect();
        assert!(types.contains(&&Type::Standard));
        assert!(types.contains(&&Type::Public));
        assert!(types.contains(&&Type::Replication));
    }

    #[tokio::test]
    async fn test_get_bucket_request_buckets_skips_tag_errors() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&["test-stack-alpha", "test-stack-bravo"]);

        let bravo_tags = bucket_tagging_xml(&[
            ("Stack", "test-stack"),
            ("BucketType", "standard"),
            ("BucketOrigin", "bucket-request"),
        ]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .s3_error("AccessDenied", "tagging denied")
            .success(bravo_tags, None)
            .build();

        let buckets = get_bucket_request_buckets(&client, &stack).await.unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name(), "test-stack-bravo");
    }
}
