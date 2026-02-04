use crate::{
    batch::{BatchError, trigger_checksum_job},
    bucket::{self, Bucket, REPLICATION_SUFFIX},
    config::{BatchConfig, RequestConfig},
};

/// Trigger S3 batch compute checksum jobs
pub async fn perform(
    batch: &BatchConfig,
    request: &RequestConfig,
) -> Result<Vec<String>, BatchError> {
    tracing::info!("Retrieving buckets for checksum report");

    let bucket_pairs = match &batch.bucket {
        Some(bucket_name) => {
            let source_name = bucket_name.as_str();
            let replication_name = format!("{}{}", source_name, REPLICATION_SUFFIX);

            let source = Bucket::from_name(&request.client, source_name)
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
            let (source_buckets, replication_buckets) = tokio::try_join!(
                bucket::get_stack_buckets_by_type(
                    &request.client,
                    request.stack(),
                    &[bucket::Type::Public, bucket::Type::Standard],
                ),
                bucket::get_stack_buckets_by_type(
                    &request.client,
                    request.stack(),
                    &[bucket::Type::Replication],
                ),
            )?;

            bucket::pair_buckets(source_buckets, replication_buckets)?
        }
    };

    let mut receipts = vec![];
    let mut issues = vec![];

    for (source, replication) in &bucket_pairs {
        match trigger_checksum_job(batch, request, source, replication).await {
            Ok(urls) => receipts.extend(urls),
            Err(e) => issues.push(e.to_string()),
        }
    }

    if !issues.is_empty() {
        return Err(BatchError::PartialFailure(issues));
    }

    Ok(receipts)
}
