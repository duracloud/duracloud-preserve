use crate::{batch, bucket as app_bucket, config::Config, errors::ComputeChecksumsError};
use awsutils::{
    batch as aws_batch,
    bucket::{self, Bucket},
};
use base::bucket::{BucketPair, Name};
use constants::REPLICATION_SUFFIX;

/// Trigger S3 batch compute checksum jobs
pub async fn perform(
    config: &Config,
    bucket: Option<&Name>,
) -> Result<Vec<String>, ComputeChecksumsError> {
    tracing::info!("Retrieving buckets for checksum report");

    let bucket_pairs = match bucket {
        Some(bucket_name) => {
            let source_name = bucket_name.as_str();
            let replication_name = format!("{source_name}{REPLICATION_SUFFIX}");

            let source = bucket::from_name(config.s3(), source_name)
                .await
                .map_err(ComputeChecksumsError::BucketDiscovery)?
                .filter(|b| {
                    matches!(
                        b.bucket_type(),
                        bucket::Type::Public | bucket::Type::Standard
                    )
                })
                .ok_or_else(|| ComputeChecksumsError::InvalidBucket(source_name.to_string()))?;

            let replication =
                Bucket::new(&replication_name, bucket::Type::Replication).map_err(|source| {
                    ComputeChecksumsError::ReplicationBucket {
                        bucket: replication_name.clone(),
                        source,
                    }
                })?;

            vec![BucketPair::new(source, replication)]
        }
        None => {
            let all_buckets = app_bucket::list_for_stack(config.s3(), config.stack(), None)
                .await
                .map_err(ComputeChecksumsError::BucketDiscovery)?;
            let (mut source_buckets, mut replication_buckets) = (Vec::new(), Vec::new());

            for bucket in all_buckets {
                match bucket.bucket_type() {
                    bucket::Type::Public | bucket::Type::Standard => source_buckets.push(bucket),
                    bucket::Type::Replication => replication_buckets.push(bucket),
                    _ => {}
                }
            }

            app_bucket::make_pairs(source_buckets, replication_buckets)
                .map_err(ComputeChecksumsError::PairBuckets)?
        }
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

        let err = perform(&config, Some(&bucket_name))
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

        let err = perform(&config, Some(&bucket_name))
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

        let err = perform(&config, None)
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

        let receipts = perform(&config, None)
            .await
            .expect("no source buckets should return empty receipts");

        assert!(receipts.is_empty());
    }
}
