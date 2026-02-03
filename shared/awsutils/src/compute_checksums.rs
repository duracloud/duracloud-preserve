use apputils::stack::DateCtx;

use crate::{
    batch::{BatchError, ChecksumJobReceipt, create_checksum_job, upload_receipt},
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

    for (source, replication) in &bucket_pairs {
        tracing::info!(
            "Processing bucket pair: {} -> {}",
            source.name(),
            replication.name()
        );

        let source_result = create_checksum_job(
            &batch.client,
            batch.account_id(),
            batch.role_arn(),
            source.name(),
            batch.stack().managed_bucket().as_str(),
        )
        .await?;

        let replication_result = create_checksum_job(
            &batch.client,
            batch.account_id(),
            batch.role_arn(),
            replication.name(),
            batch.stack().managed_bucket().as_str(),
        )
        .await?;

        let receipt = ChecksumJobReceipt::new(
            source.name(),
            &source_result,
            replication.name(),
            &replication_result,
        );

        let stack = request.stack();
        let paths = vec![
            stack.metadata_checksums_path(&source_result, DateCtx::Latest),
            stack.metadata_checksums_path(&replication_result, DateCtx::Latest),
            stack.metadata_checksums_path(source.name(), DateCtx::Latest),
            stack.metadata_checksums_path(source.name(), DateCtx::Today),
        ];

        tracing::info!("Uploading receipt: {:?}", receipt);

        let urls =
            upload_receipt(&request.client, &stack.managed_bucket(), &receipt, &paths).await?;

        receipts.extend(urls);
    }

    Ok(receipts)
}
