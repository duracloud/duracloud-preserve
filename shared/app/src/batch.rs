use apputils::{content_type::APPLICATION_JSON, stack::DateCtx};
use aws_sdk_s3control::types::JobStatus;

use awsutils::{
    batch::{self as aws_batch, BatchManifest, ChecksumJobReceipt},
    bucket::Bucket,
    file::{self, File},
};

use crate::{config::Config, errors::BatchStatusError, upload::upload_bytes};

/// Download a batch manifest
pub async fn get_batch_manifest(
    config: &Config,
    bucket: &str,
    job_id: &str,
) -> Result<BatchManifest, BatchStatusError> {
    let managed_bucket = config.stack().managed_bucket();
    let manifest = &File::new(
        &managed_bucket,
        config
            .stack()
            .batch_reports_checksum_manifest(bucket, job_id),
    );

    if !file::exists(config.s3(), manifest).await {
        tracing::info!("Manifest not found: {}", manifest.s3_url());
        return Err(BatchStatusError::ManifestNotFound(manifest.s3_url()));
    }

    BatchManifest::fetch(config.s3(), manifest)
        .await
        .map_err(BatchStatusError::from)
}

/// Get a batch job's current status
pub async fn get_job_status(config: &Config, job_id: &str) -> Result<JobStatus, BatchStatusError> {
    let resp = config
        .s3control()
        .describe_job()
        .account_id(config.account_id())
        .job_id(job_id)
        .send()
        .await
        .map_err(|e| {
            BatchStatusError::Batch(aws_batch::BatchError::S3Control(Box::new(
                aws_batch::s3control_error("DescribeJob", &e),
            )))
        })?;

    let job = resp
        .job
        .ok_or(BatchStatusError::JobNotFound(job_id.to_string()))?;

    let status = job
        .status
        .ok_or(BatchStatusError::MissingStatus(job_id.to_string()))?;

    Ok(status)
}

/// Get a batch job manifest if it's available (job is complete and file is present)
pub async fn get_manifest_if_ready(
    config: &Config,
    bucket: &str,
    job_id: &str,
) -> Result<Option<BatchManifest>, BatchStatusError> {
    let status = get_job_status(config, job_id).await?;

    match status {
        JobStatus::Complete => get_batch_manifest(config, bucket, job_id).await.map(Some),
        JobStatus::Failed => Err(BatchStatusError::JobFailed(job_id.to_string())),
        status => {
            tracing::info!("Job {} not in continuable status: {}", job_id, status);
            Ok(None)
        }
    }
}

/// Trigger compute checksum jobs for source and replication bucket pair
pub async fn trigger_checksum_job(
    config: &Config,
    source: &Bucket,
    replication: &Bucket,
) -> Result<Vec<String>, aws_batch::BatchError> {
    tracing::info!(
        "Processing bucket pair: {} -> {}",
        source.name(),
        replication.name()
    );

    let source_result = aws_batch::create_checksum_job(
        config.s3control(),
        config.account_id(),
        config.batch_role_arn(),
        source.name(),
        config.stack().managed_bucket().as_str(),
    )
    .await?;

    tracing::info!(
        "Created source checksum job: bucket={}, job_id={}",
        source.name(),
        source_result
    );

    let replication_result = match aws_batch::create_checksum_job(
        config.s3control(),
        config.account_id(),
        config.batch_role_arn(),
        replication.name(),
        config.stack().managed_bucket().as_str(),
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(
                "Failed to create replication job for {}: {}. Orphaned source job: {}",
                replication.name(),
                e,
                source_result
            );
            return Err(e);
        }
    };

    tracing::info!(
        "Created replication checksum job: bucket={}, job_id={}",
        replication.name(),
        replication_result
    );

    let receipt = ChecksumJobReceipt::new(
        source.name(),
        &source_result,
        replication.name(),
        &replication_result,
    );

    let stack = config.stack();
    let paths = [
        stack.metadata_checksums_receipts_path(&source_result, DateCtx::Latest),
        stack.metadata_checksums_receipts_path(&replication_result, DateCtx::Latest),
        stack.metadata_checksums_receipts_path(source.name(), DateCtx::Latest),
        stack.metadata_checksums_receipts_path(source.name(), DateCtx::Today),
    ];

    tracing::info!("Uploading receipt: {:?}", receipt);

    let managed_bucket = stack.managed_bucket();
    let files = paths.iter().map(|p| File::new(&managed_bucket, p));
    Ok(upload_bytes(
        config.s3(),
        serde_json::to_vec(&receipt)?,
        APPLICATION_JSON,
        files,
    )
    .await?)
}
