use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use aws_sdk_s3::{
    Client,
    types::{Tag, TransitionStorageClass},
};
use base::{
    Stack,
    bucket::{BucketPair, Name},
    errors::BucketValidationError,
    stack::DateCtx,
    stats::InventoryStats,
};
use constants::*;
use tokio::{fs, io};

use awsutils::{
    bucket::{self, Bucket, RequestError, Type},
    bucket_creator::{BucketCreator, BucketCreatorParams},
    errors::S3ResultExt,
    file::{self, File},
    users::UserInfo,
};

use crate::{
    config::Config,
    errors::{ComputeChecksumsError, FileKeyError, StorageReportError},
};

/// Per-stack bucket list cache for reuse when processing many users.
#[derive(Default)]
pub struct BucketCache(HashMap<String, Vec<Bucket>>);

impl BucketCache {
    pub fn new() -> Self {
        Self::default()
    }
}

type TagFilter<'a> = Option<&'a dyn Fn(&[Tag]) -> bool>;

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

/// Create a primary bucket and its replication bucket, then enable replication.
pub async fn create_pair(
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

/// Arrange creation of primary and replication buckets.
pub async fn create_pairs(
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

        let result = create_pair(
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

/// Fetch the latest inventory-stats JSON for every bucket and decode it.
pub async fn fetch_latest_inventory_stats(
    config: &Config,
    buckets: Vec<Bucket>,
) -> Result<BTreeMap<String, InventoryStats>, StorageReportError> {
    let mut bucket_stats = BTreeMap::new();

    for bucket in buckets {
        let bucket_name = bucket.name().to_string();
        let bucket_type = bucket.bucket_type();

        tracing::info!("Retrieving inventory stats for: {bucket_name} {bucket_type}");

        let stats_file = File::from(
            config
                .stack()
                .metadata_manifests_stats_path(&bucket_name, DateCtx::Latest),
        );

        if !file::exists(config.s3(), &stats_file).await {
            tracing::warn!("No stats found for: {bucket_name} {bucket_type}");
            continue;
        }

        let bytes = file::download_bytes(config.s3(), &stats_file)
            .await
            .map_err(|source| StorageReportError::DownloadStats {
                bucket: bucket_name.clone(),
                source,
            })?;

        let stats: InventoryStats =
            serde_json::from_slice(&bytes).map_err(|source| StorageReportError::ParseStats {
                bucket: bucket_name.clone(),
                source,
            })?;

        bucket_stats.insert(bucket_name, stats);
    }

    Ok(bucket_stats)
}

/// Read prospective bucket names limited to the bucket request max
pub async fn get_request_names(file: PathBuf) -> Result<Vec<String>, io::Error> {
    Ok(parse_request_names(&fs::read_to_string(file).await?))
}

/// Get stack buckets that were created via bucket-request (BucketOrigin=bucket-request).
/// These are the buckets eligible for reconciliation.
pub async fn get_requested(client: &Client, stack: &Stack) -> Result<Vec<Bucket>, RequestError> {
    list_for_stack(
        client,
        stack,
        Some(&|tags: &[Tag]| {
            tags.iter().any(|tag| {
                tag.key() == BUCKET_TAG_ORIGIN_KEY && tag.value() == BUCKET_TAG_ORIGIN_VAL
            })
        }),
    )
    .await
}

/// Get buckets belonging to a stack (prefix match + stack tag) with optional filter.
pub async fn list_for_stack(
    client: &Client,
    stack: &Stack,
    filter: TagFilter<'_>,
) -> Result<Vec<Bucket>, RequestError> {
    let prefix = format!("{}-", stack.as_str());
    let mut buckets = Vec::new();

    let response = client
        .list_buckets()
        .send()
        .await
        .s3_err("failed to list buckets")?;

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

        if let Some(found) = filter
            && !found(tags)
        {
            continue;
        }

        if let Some(bucket) = bucket::from_tags(name, tags)? {
            buckets.push(bucket);
        }
    }

    Ok(buckets)
}

/// Get stack buckets filtered by type.
pub async fn list_for_stack_by_type(
    client: &Client,
    stack: &Stack,
    types: &[Type],
) -> Result<Vec<Bucket>, RequestError> {
    if types.is_empty() {
        return Ok(vec![]);
    }

    list_for_stack(
        client,
        stack,
        Some(&|tags: &[Tag]| bucket::type_from_tags(tags).is_some_and(|t| types.contains(&t))),
    )
    .await
}

/// List buckets accessible to a user across every stack named by their IAM groups.
///
/// Groups that don't parse as stack names are skipped (logged at debug).
/// Only `Internal`, `Public`, and `Standard` buckets are returned.
pub async fn list_for_user_stacks(
    client: &Client,
    user: &UserInfo,
    cache: &mut BucketCache,
) -> Result<Vec<Bucket>, RequestError> {
    let stacks: Vec<Stack> = user
        .groups
        .iter()
        .filter_map(|group| match Stack::from_prefixed_name(group) {
            Ok(stack) => Some(stack),
            Err(reason) => {
                tracing::debug!("Skipping group '{group}' (not a stack name): {reason}");
                None
            }
        })
        .collect();

    let mut buckets = Vec::new();
    for stack in &stacks {
        let stack_buckets = match cache.0.get(stack.as_str()) {
            Some(cached) => cached.clone(),
            None => {
                let fetched = list_for_stack_by_type(
                    client,
                    stack,
                    &[Type::Internal, Type::Public, Type::Standard],
                )
                .await?;
                cache.0.insert(stack.as_str().to_string(), fetched.clone());
                fetched
            }
        };
        buckets.extend(stack_buckets);
    }

    Ok(buckets)
}

/// Pair source buckets with their replication buckets.
/// Returns an error if any source bucket lacks a matching replication bucket.
pub fn make_pairs(
    source_buckets: Vec<Bucket>,
    replication_buckets: Vec<Bucket>,
) -> Result<Vec<BucketPair>, BucketValidationError> {
    let mut repl_map: HashMap<String, Bucket> = replication_buckets
        .into_iter()
        .filter_map(|b| {
            let name = b.name().to_string();
            name.strip_suffix(REPLICATION_SUFFIX)
                .map(|base| (base.to_string(), b))
        })
        .collect();

    source_buckets
        .into_iter()
        .map(|source| {
            let source_name = source.name().to_string();
            repl_map
                .remove(&source_name)
                .map(|repl| BucketPair::new(source, repl))
                .ok_or_else(|| {
                    BucketValidationError::ValidationError(format!(
                        "no replication bucket found for '{}'",
                        source_name
                    ))
                })
        })
        .collect()
}

/// Extract the name from an object key.
/// Key format example: `reports/{date_ctx}/manifests/{bucket}.csv`
pub fn name_from_file(file: &File) -> Result<&str, FileKeyError> {
    file.key()
        .rsplit('/')
        .next()
        .and_then(|filename| filename.rsplit_once('.').map(|(bucket, _)| bucket))
        .ok_or(FileKeyError::MissingExtension(file.key().to_string()))
}

/// Parse prospective bucket names limited to the bucket request max
pub fn parse_request_names(content: &str) -> Vec<String> {
    content
        .lines()
        .map(String::from)
        .take(MAX_BUCKETS_PER_REQUEST as usize)
        .collect()
}

/// Retrieve bucket request file and verify it is valid.
pub async fn read_request_names(client: &Client, file: &File) -> Result<Vec<String>, RequestError> {
    let Ok(r) = file::download(client, file).await else {
        return Err(RequestError::S3Error("failed to download file".to_string()));
    };

    if let Some(ct) = r.content_type()
        && !ct.starts_with(TEXT_PLAIN)
    {
        return Err(RequestError::InvalidContentType);
    }

    if let Some(len) = r.content_length()
        && len > MAX_REQUEST_FILE_SIZE as i64
    {
        return Err(RequestError::FileTooLarge {
            actual: len,
            max: MAX_REQUEST_FILE_SIZE as i64,
        });
    }

    let bytes = r
        .body
        .collect()
        .await
        .s3_err("failed to read response body")?
        .into_bytes();
    let content = String::from_utf8_lossy(&bytes);
    let names = parse_request_names(&content);

    Ok(names)
}

/// Resolve all source buckets in a stack into checksum source/replication pairs.
pub async fn resolve_all_checksum_pairs(
    client: &Client,
    stack: &Stack,
) -> Result<Vec<BucketPair>, ComputeChecksumsError> {
    let all_buckets = list_for_stack(client, stack, None)
        .await
        .map_err(ComputeChecksumsError::BucketDiscovery)?;

    let (mut source_buckets, mut replication_buckets) = (Vec::new(), Vec::new());
    for bucket in all_buckets {
        match bucket.bucket_type() {
            Type::Public | Type::Standard => source_buckets.push(bucket),
            Type::Replication => replication_buckets.push(bucket),
            _ => {}
        }
    }

    make_pairs(source_buckets, replication_buckets).map_err(ComputeChecksumsError::PairBuckets)
}

/// Resolve a single source bucket name into a checksum source/replication pair.
/// Rejects buckets that aren't `Public` or `Standard` type.
pub async fn resolve_checksum_pair_by_name(
    client: &Client,
    name: &Name,
) -> Result<BucketPair, ComputeChecksumsError> {
    let source_name = name.as_str();
    let replication_name = format!("{source_name}{REPLICATION_SUFFIX}");

    let source = bucket::from_name(client, source_name)
        .await
        .map_err(ComputeChecksumsError::BucketDiscovery)?
        .filter(|b| matches!(b.bucket_type(), Type::Public | Type::Standard))
        .ok_or_else(|| ComputeChecksumsError::InvalidBucket(source_name.to_string()))?;

    let replication = Bucket::new(&replication_name, Type::Replication).map_err(|source| {
        ComputeChecksumsError::ReplicationBucket {
            bucket: replication_name.clone(),
            source,
        }
    })?;

    Ok(BucketPair::new(source, replication))
}

/// Check that user supplied bucket names are valid and convert
/// to (primary, replication) pairs for the stack.
pub fn review_request_names(
    stack: &Stack,
    names: &[String],
) -> Result<Vec<BucketPair>, BucketValidationError> {
    let mut buckets: Vec<BucketPair> = Vec::new();

    for name in names {
        let partial = Name::new(name)?;
        let primary = Bucket::primary(stack, &partial)?;
        let replication = Bucket::replication(stack, &partial)?;
        buckets.push(BucketPair::new(primary, replication));
    }

    Ok(buckets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use constants::{APPLICATION_JSON, MAX_BUCKETS_PER_REQUEST, TEXT_PLAIN};
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
    async fn test_get_requested_filters_by_origin_tag() {
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

        let buckets = get_requested(&client, &stack).await.unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name(), "test-stack-alpha");
        assert_eq!(buckets[0].bucket_type(), &Type::Standard);
    }

    #[tokio::test]
    async fn test_get_requested_includes_multiple_types() {
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

        let buckets = get_requested(&client, &stack).await.unwrap();

        assert_eq!(buckets.len(), 3);
        let types: Vec<&Type> = buckets.iter().map(|b| b.bucket_type()).collect();
        assert!(types.contains(&&Type::Standard));
        assert!(types.contains(&&Type::Public));
        assert!(types.contains(&&Type::Replication));
    }

    #[tokio::test]
    async fn test_get_requested_skips_tag_errors() {
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

        let buckets = get_requested(&client, &stack).await.unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name(), "test-stack-bravo");
    }

    #[tokio::test]
    async fn test_list_for_stack_by_type_empty_types_short_circuits() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&["test-stack-public", "test-stack-repl"]);

        let (client, replay) = TestClientBuilder::new()
            .success(list, None)
            .build_with_replay();

        let buckets = list_for_stack_by_type(&client, &stack, &[]).await.unwrap();

        assert!(buckets.is_empty());
        assert!(test_support::recorded_requests(&replay).is_empty());
    }

    #[tokio::test]
    async fn test_list_for_stack_by_type_filters_results() {
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

        let buckets = list_for_stack_by_type(&client, &stack, &[Type::Public])
            .await
            .unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name(), "test-stack-public");
        assert_eq!(buckets[0].bucket_type(), &Type::Public);
    }

    #[tokio::test]
    async fn test_list_for_stack_excludes_missing_or_invalid_bucket_type() {
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

        let buckets = list_for_stack(&client, &stack, None).await.unwrap();
        assert!(buckets.is_empty());
    }

    #[tokio::test]
    async fn test_list_for_stack_excludes_non_matching_stack_tag() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&["test-stack-alpha"]);
        let tags = bucket_tagging_xml(&[("Stack", "other-stack"), ("BucketType", "standard")]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .success(tags, None)
            .build();

        let buckets = list_for_stack(&client, &stack, None).await.unwrap();
        assert!(buckets.is_empty());
    }

    #[tokio::test]
    async fn test_list_for_stack_skips_tag_lookup_failures() {
        let stack = Stack::new("test-stack").unwrap();
        let list = list_buckets_xml(&["test-stack-alpha", "test-stack-bravo"]);
        let bravo_tags = bucket_tagging_xml(&[("Stack", "test-stack"), ("BucketType", "standard")]);

        let client = TestClientBuilder::new()
            .success(list, None)
            .s3_error("AccessDenied", "tagging denied")
            .success(bravo_tags, None)
            .build();

        let buckets = list_for_stack(&client, &stack, None).await.unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name(), "test-stack-bravo");
        assert_eq!(buckets[0].bucket_type(), &Type::Standard);
    }

    #[test]
    fn test_make_pairs() {
        let source_buckets = vec![
            Bucket::new("alpha", Type::Standard).unwrap(),
            Bucket::new("beta", Type::Public).unwrap(),
        ];
        let replication_buckets = vec![
            Bucket::new("beta-repl", Type::Replication).unwrap(),
            Bucket::new("alpha-repl", Type::Replication).unwrap(),
        ];

        let pairs = make_pairs(source_buckets, replication_buckets).unwrap();

        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].source.name(), "alpha");
        assert_eq!(pairs[0].replication.name(), "alpha-repl");
        assert_eq!(pairs[1].source.name(), "beta");
        assert_eq!(pairs[1].replication.name(), "beta-repl");
    }

    #[test]
    fn test_make_pairs_missing_replication() {
        let source_buckets = vec![
            Bucket::new("alpha", Type::Standard).unwrap(),
            Bucket::new("beta", Type::Public).unwrap(),
        ];
        let replication_buckets = vec![
            Bucket::new("alpha-repl", Type::Replication).unwrap(),
            // missing beta-repl
        ];

        let result = make_pairs(source_buckets, replication_buckets);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, BucketValidationError::ValidationError(_)));
        assert!(err.to_string().contains("beta"));
    }

    #[tokio::test]
    async fn test_name_from_file() {
        let file = File::new("managed", "reports/latest/manifests/my-bucket.csv");
        assert_eq!(name_from_file(&file).unwrap(), "my-bucket");
    }

    #[tokio::test]
    async fn test_name_from_file_invalid() {
        let file = File::new("managed", "reports/latest/manifests/no-extension");
        assert!(name_from_file(&file).is_err());
    }

    #[tokio::test]
    async fn test_read_request_names() {
        let content = "123\n456\n789\n234\n567\n890";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some(TEXT_PLAIN.to_string()))
            .build();

        let names = read_request_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), 5);
        assert_eq!(names[0], "123");
        assert_eq!(names[1], "456");
        assert_eq!(names[2], "789");
        assert_eq!(names[3], "234");
    }

    #[tokio::test]
    async fn test_read_request_names_content_type_with_charset() {
        let content = "bucket1\nbucket2";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some(TEXT_PLAIN.to_string()))
            .build();

        let names = read_request_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "bucket1");
        assert_eq!(names[1], "bucket2");
    }

    #[tokio::test]
    async fn test_read_request_names_exceeds_size_limit() {
        let content = "a".repeat((MAX_REQUEST_FILE_SIZE + 1) as usize);
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some(TEXT_PLAIN.to_string()))
            .build();

        let result = read_request_names(&client, &file).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::FileTooLarge { actual, max }) => {
                assert_eq!(actual, (MAX_REQUEST_FILE_SIZE + 1) as i64);
                assert_eq!(max, MAX_REQUEST_FILE_SIZE as i64);
            }
            _ => panic!("Expected FileTooLarge error"),
        }
    }

    #[tokio::test]
    async fn test_read_request_names_invalid_content_type() {
        let content = "bucket1\nbucket2";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some(APPLICATION_JSON.to_string()))
            .build();

        let result = read_request_names(&client, &file).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(RequestError::InvalidContentType)));
    }

    #[tokio::test]
    async fn test_read_request_names_truncates_at_max() {
        let content = "a\nb\nc\nd\ne\nf\ng";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some(TEXT_PLAIN.to_string()))
            .build();

        let names = read_request_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), MAX_BUCKETS_PER_REQUEST as usize);
    }

    #[test]
    fn test_review_request_names() {
        let stack = Stack::new("test-stack").unwrap();

        let names = vec!["example".to_string(), "data-public".to_string()];

        let result = review_request_names(&stack, &names).unwrap();

        assert_eq!(result.len(), 2);

        // First bucket pair (standard)
        assert_eq!(result[0].source.name(), "test-stack-example");
        assert_eq!(result[0].source.bucket_type(), &Type::Standard);
        assert_eq!(result[0].replication.name(), "test-stack-example-repl");
        assert_eq!(result[0].replication.bucket_type(), &Type::Replication);

        // Second bucket pair (public)
        assert_eq!(result[1].source.name(), "test-stack-data-public");
        assert_eq!(result[1].source.bucket_type(), &Type::Public);
        assert_eq!(result[1].replication.name(), "test-stack-data-public-repl");
        assert_eq!(result[1].replication.bucket_type(), &Type::Replication);
    }
}
