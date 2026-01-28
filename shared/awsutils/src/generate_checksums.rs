use crate::{
    batch::{BatchError, ChecksumJobReceipt, create_checksum_job},
    bucket::{self},
    config::{BatchConfig, RequestConfig},
    file::{self, File},
};
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;

/// Trigger S3 batch operation jobs for generating checksum reports
pub async fn perform(
    batch: &BatchConfig,
    request: &RequestConfig,
) -> Result<Vec<String>, BatchError> {
    tracing::info!("Retrieving buckets for checksum report");

    let source_buckets = bucket::get_stack_buckets_by_type(
        &request.client,
        request.stack(),
        &[bucket::Type::Public, bucket::Type::Standard],
    )
    .await?;

    let replication_buckets = bucket::get_stack_buckets_by_type(
        &request.client,
        request.stack(),
        &[bucket::Type::Replication],
    )
    .await?;

    let bucket_pairs = bucket::pair_buckets(source_buckets, replication_buckets)?;
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

        let job = ChecksumJobReceipt::new(
            source.name(),
            &source_result,
            replication.name(),
            &replication_result,
        );

        let file = &File::new(
            request.stack().managed_bucket(),
            request
                .stack()
                .reports_checksums_path(source.name(), apputils::stack::DateCtx::Latest),
        );

        file::upload(
            &request.client,
            file,
            ByteStream::from(Bytes::from(serde_json::to_vec(&job)?)),
            "application/json",
        )
        .await?;

        receipts.push(file.http_url());
    }

    Ok(receipts)
}
