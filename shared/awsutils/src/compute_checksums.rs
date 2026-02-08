use crate::{
    batch::{BatchError, trigger_checksum_job},
    bucket::{self, Bucket, Name, REPLICATION_SUFFIX},
    config::Config,
};
use futures::future::BoxFuture;

/// Trigger S3 batch compute checksum jobs
pub async fn perform(config: &Config, bucket: Option<&Name>) -> Result<Vec<String>, BatchError> {
    tracing::info!("Retrieving buckets for checksum report");

    let bucket_pairs = match bucket {
        Some(bucket_name) => {
            let source_name = bucket_name.as_str();
            let replication_name = format!("{}{}", source_name, REPLICATION_SUFFIX);

            let source = Bucket::from_name(config.s3(), source_name)
                .await?
                .filter(|b| {
                    matches!(
                        b.bucket_type(),
                        bucket::Type::Public | bucket::Type::Standard
                    )
                })
                .ok_or_else(|| BatchError::InvalidBucket(source_name.to_string()))?;

            let replication = Bucket::new(&replication_name, bucket::Type::Replication)?;

            vec![(source, replication)]
        }
        None => {
            let all_buckets = bucket::get_stack_buckets(config.s3(), config.stack()).await?;
            let (mut source_buckets, mut replication_buckets) = (Vec::new(), Vec::new());

            for bucket in all_buckets {
                match bucket.bucket_type() {
                    bucket::Type::Public | bucket::Type::Standard => source_buckets.push(bucket),
                    bucket::Type::Replication => replication_buckets.push(bucket),
                    _ => {}
                }
            }

            bucket::pair_buckets(source_buckets, replication_buckets)?
        }
    };

    dispatch_checksum_jobs(config, &bucket_pairs, |cfg, source, replication| {
        Box::pin(trigger_checksum_job(cfg, source, replication))
    })
    .await
}

async fn dispatch_checksum_jobs<F>(
    config: &Config,
    bucket_pairs: &[(Bucket, Bucket)],
    trigger: F,
) -> Result<Vec<String>, BatchError>
where
    F: for<'a> Fn(
        &'a Config,
        &'a Bucket,
        &'a Bucket,
    ) -> BoxFuture<'a, Result<Vec<String>, BatchError>>,
{
    let mut receipts = vec![];
    let mut issues = vec![];

    for (source, replication) in bucket_pairs {
        match trigger(config, source, replication).await {
            Ok(urls) => receipts.extend(urls),
            Err(e) => issues.push(e.to_string()),
        }
    }

    if !issues.is_empty() {
        return Err(BatchError::PartialFailure(issues));
    }

    Ok(receipts)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::{
        bucket::RequestError,
        test_client::{TestClientBuilder, test_config_with_client},
    };

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

    fn bucket_pair(name: &str, bucket_type: bucket::Type) -> (Bucket, Bucket) {
        let source = Bucket::new(name, bucket_type).expect("source should be valid");
        let replication = Bucket::new(
            format!("{name}{REPLICATION_SUFFIX}").as_str(),
            bucket::Type::Replication,
        )
        .expect("replication should be valid");
        (source, replication)
    }

    #[tokio::test]
    async fn test_perform_specific_bucket_returns_invalid_bucket_when_tag_lookup_non_success() {
        let bucket_name = Name::new("test-stack-missing").unwrap();
        let client = TestClientBuilder::new()
            .s3_error("NoSuchBucket", "bucket not found")
            .build();
        let config = test_config_with_client(client);

        let err = perform(&config, Some(&bucket_name))
            .await
            .expect_err("missing bucket should be invalid");

        match err {
            BatchError::InvalidBucket(name) => assert_eq!(name, "test-stack-missing"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_perform_specific_bucket_rejects_non_source_bucket_type() {
        let bucket_name = Name::new("test-stack-alpha-repl").unwrap();
        let tagging = bucket_tagging_xml(&[("BucketType", "replication")]);
        let client = TestClientBuilder::new().success(tagging, None).build();
        let config = test_config_with_client(client);

        let err = perform(&config, Some(&bucket_name))
            .await
            .expect_err("replication bucket should be invalid");

        match err {
            BatchError::InvalidBucket(name) => assert_eq!(name, "test-stack-alpha-repl"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_perform_stack_mode_errors_when_replication_pair_missing() {
        let buckets = list_buckets_xml(&["test-stack-alpha"]);
        let tags = bucket_tagging_xml(&[("Stack", "test-stack"), ("BucketType", "standard")]);
        let client = TestClientBuilder::new()
            .success(buckets, None)
            .success(tags, None)
            .build();
        let config = test_config_with_client(client);

        let err = perform(&config, None)
            .await
            .expect_err("missing replication pair should fail");

        match err {
            BatchError::Request(RequestError::S3Error(msg)) => {
                assert!(msg.contains("no replication bucket found"));
                assert!(msg.contains("test-stack-alpha"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_perform_stack_mode_returns_empty_when_no_source_buckets() {
        let client = TestClientBuilder::new()
            .success(list_buckets_xml(&[]), None)
            .build();
        let config = test_config_with_client(client);

        let receipts = perform(&config, None)
            .await
            .expect("no source buckets should return empty receipts");

        assert!(receipts.is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_checksum_jobs_flattens_receipts_in_pair_order() {
        let bucket_pairs = vec![
            bucket_pair("test-stack-alpha", bucket::Type::Standard),
            bucket_pair("test-stack-bravo-public", bucket::Type::Public),
        ];
        let config = test_config_with_client(TestClientBuilder::new().build());

        let receipts = dispatch_checksum_jobs(&config, &bucket_pairs, |_cfg, source, _repl| {
            let source_name = source.name().to_string();
            Box::pin(async move {
                Ok(vec![
                    format!("https://example.local/{source_name}/latest"),
                    format!("https://example.local/{source_name}/today"),
                ])
            })
        })
        .await
        .expect("dispatch should succeed");

        assert_eq!(
            receipts,
            vec![
                "https://example.local/test-stack-alpha/latest".to_string(),
                "https://example.local/test-stack-alpha/today".to_string(),
                "https://example.local/test-stack-bravo-public/latest".to_string(),
                "https://example.local/test-stack-bravo-public/today".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_dispatch_checksum_jobs_aggregates_partial_failures() {
        let bucket_pairs = vec![
            bucket_pair("test-stack-alpha", bucket::Type::Standard),
            bucket_pair("test-stack-bravo-public", bucket::Type::Public),
            bucket_pair("test-stack-charlie", bucket::Type::Standard),
        ];
        let config = test_config_with_client(TestClientBuilder::new().build());

        let err = dispatch_checksum_jobs(&config, &bucket_pairs, |_cfg, source, _repl| {
            let source_name = source.name().to_string();
            Box::pin(async move {
                if source_name == "test-stack-bravo-public" || source_name == "test-stack-charlie" {
                    Err(BatchError::InvalidBucket(source_name))
                } else {
                    Ok(vec![format!("https://example.local/{source_name}/latest")])
                }
            })
        })
        .await
        .expect_err("dispatch should return partial failure");

        match err {
            BatchError::PartialFailure(issues) => {
                assert_eq!(issues.len(), 2);
                assert!(issues.iter().any(|m| m.contains("test-stack-bravo-public")));
                assert!(issues.iter().any(|m| m.contains("test-stack-charlie")));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_dispatch_checksum_jobs_does_not_short_circuit_after_failure() {
        let bucket_pairs = vec![
            bucket_pair("test-stack-alpha", bucket::Type::Standard),
            bucket_pair("test-stack-bravo-public", bucket::Type::Public),
            bucket_pair("test-stack-charlie", bucket::Type::Standard),
        ];
        let config = test_config_with_client(TestClientBuilder::new().build());
        let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let err = dispatch_checksum_jobs(&config, &bucket_pairs, {
            let calls = Arc::clone(&calls);
            move |_cfg, source, _repl| {
                let calls = Arc::clone(&calls);
                let source_name = source.name().to_string();
                Box::pin(async move {
                    calls.lock().unwrap().push(source_name.clone());
                    if source_name == "test-stack-alpha" {
                        Err(BatchError::InvalidBucket(source_name))
                    } else {
                        Ok(vec![format!("https://example.local/{source_name}/latest")])
                    }
                })
            }
        })
        .await
        .expect_err("dispatch should fail due to first bucket");

        match err {
            BatchError::PartialFailure(issues) => {
                assert_eq!(issues.len(), 1);
                assert!(issues[0].contains("test-stack-alpha"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let seen = calls.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![
                "test-stack-alpha".to_string(),
                "test-stack-bravo-public".to_string(),
                "test-stack-charlie".to_string(),
            ]
        );
    }
}
