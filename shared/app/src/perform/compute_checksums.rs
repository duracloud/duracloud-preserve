use crate::{batch, bucket as app_bucket, config::Config, errors::ComputeChecksumsError};
use awsutils::batch as aws_batch;
use base::bucket::Name;

/// Trigger S3 batch compute checksum jobs
#[derive(Debug, Clone, Default)]
pub struct PerformArgs {
    pub bucket: Option<Name>,
}

impl PerformArgs {
    pub fn for_bucket(bucket: Name) -> Self {
        Self {
            bucket: Some(bucket),
        }
    }
}

pub async fn perform(
    config: &Config,
    args: &PerformArgs,
) -> Result<Vec<String>, ComputeChecksumsError> {
    tracing::info!("Retrieving buckets for checksum report");

    let bucket_pairs = match args.bucket.as_ref() {
        Some(name) => vec![app_bucket::resolve_checksum_pair_by_name(config.s3(), name).await?],
        None => app_bucket::resolve_all_checksum_pairs(config.s3(), config.stack()).await?,
    };

    aws_batch::dispatch_bucket_pair_jobs(&bucket_pairs, |source, replication| {
        Box::pin(batch::trigger_checksum_job(config, source, replication))
    })
    .await
    .map_err(ComputeChecksumsError::PartialFailure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config as app_config;
    use base::errors::BucketValidationError;
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
    async fn test_perform_specific_bucket_rejects_non_source_bucket_type() {
        let bucket_name = Name::new("test-stack-alpha-repl").unwrap();
        let tagging = bucket_tagging_xml(&[("BucketType", "replication")]);
        let sdk_config = TestClientBuilder::new()
            .success(tagging, None)
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);
        let args = PerformArgs::for_bucket(bucket_name);

        let err = perform(&config, &args)
            .await
            .expect_err("replication bucket should be invalid");

        match err {
            ComputeChecksumsError::InvalidBucket(name) => {
                assert_eq!(name, "test-stack-alpha-repl")
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_perform_specific_bucket_returns_invalid_bucket_when_tag_lookup_non_success() {
        let bucket_name = Name::new("test-stack-missing").unwrap();
        let sdk_config = TestClientBuilder::new()
            .s3_error("NoSuchBucket", "bucket not found")
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);
        let args = PerformArgs::for_bucket(bucket_name);

        let err = perform(&config, &args)
            .await
            .expect_err("missing bucket should be invalid");

        match err {
            ComputeChecksumsError::InvalidBucket(name) => assert_eq!(name, "test-stack-missing"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_perform_stack_mode_errors_when_replication_pair_missing() {
        let buckets = list_buckets_xml(&["test-stack-alpha"]);
        let tags = bucket_tagging_xml(&[("Stack", "test-stack"), ("BucketType", "standard")]);
        let sdk_config = TestClientBuilder::new()
            .success(buckets, None)
            .success(tags, None)
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);
        let args = PerformArgs::default();

        let err = perform(&config, &args)
            .await
            .expect_err("missing replication pair should fail");

        match err {
            ComputeChecksumsError::PairBuckets(BucketValidationError::ValidationError(msg)) => {
                assert!(msg.contains("no replication bucket found"));
                assert!(msg.contains("test-stack-alpha"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_perform_stack_mode_returns_empty_when_no_source_buckets() {
        let sdk_config = TestClientBuilder::new()
            .success(list_buckets_xml(&[]), None)
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);
        let args = PerformArgs::default();

        let receipts = perform(&config, &args)
            .await
            .expect("no source buckets should return empty receipts");

        assert!(receipts.is_empty());
    }
}
