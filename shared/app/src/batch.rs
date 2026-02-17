use apputils::stack::DateCtx;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3control::types::JobStatus;
use bytes::Bytes;
use futures::future::try_join_all;

use awsutils::{
    batch::{self as aws_batch, BatchError, BatchManifest, ChecksumJobReceipt},
    bucket::{Bucket, RequestError},
    file::{self, File},
};

use crate::config::Config;

/// Download a batch manifest
pub async fn get_batch_manifest(
    config: &Config,
    bucket: &str,
    job_id: &str,
) -> Result<BatchManifest, BatchError> {
    let managed_bucket = config.stack().managed_bucket();
    let manifest = &File::new(
        &managed_bucket,
        config
            .stack()
            .batch_reports_checksum_manifest(bucket, job_id),
    );

    if !file::exists(config.s3(), manifest).await {
        tracing::info!("Manifest not found: {}", manifest.s3_url());
        return Err(BatchError::ManifestNotFound(manifest.s3_url()));
    }

    BatchManifest::fetch(config.s3(), manifest).await
}

/// Get a batch job's current status
pub async fn get_job_status(config: &Config, job_id: &str) -> Result<JobStatus, BatchError> {
    let resp = config
        .s3control()
        .describe_job()
        .account_id(config.account_id())
        .job_id(job_id)
        .send()
        .await
        .map_err(|e| {
            BatchError::S3Control(Box::new(aws_batch::s3control_error("DescribeJob", &e)))
        })?;

    let job = resp
        .job
        .ok_or(BatchError::JobNotFound(job_id.to_string()))?;

    let status = job
        .status
        .ok_or(BatchError::MissingStatus(job_id.to_string()))?;

    Ok(status)
}

/// Get a batch job manifest if it's available (job is complete and file is present)
pub async fn get_manifest_if_ready(
    config: &Config,
    bucket: &str,
    job_id: &str,
) -> Result<Option<BatchManifest>, RequestError> {
    let status = get_job_status(config, job_id)
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to get job status: {}", e)))?;

    match status {
        JobStatus::Complete => match get_batch_manifest(config, bucket, job_id).await {
            Ok(manifest) => Ok(Some(manifest)),
            Err(e) => Err(RequestError::S3Error(e.to_string())),
        },
        JobStatus::Failed => Err(RequestError::S3Error(format!("job {} failed", job_id))),
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
) -> Result<Vec<String>, BatchError> {
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
    let paths = vec![
        stack.metadata_checksums_path(&source_result, DateCtx::Latest),
        stack.metadata_checksums_path(&replication_result, DateCtx::Latest),
        stack.metadata_checksums_path(source.name(), DateCtx::Latest),
        stack.metadata_checksums_path(source.name(), DateCtx::Today),
    ];

    tracing::info!("Uploading receipt: {:?}", receipt);

    upload_receipt(config.s3(), &stack.managed_bucket(), &receipt, &paths).await
}

/// Uploads a receipt to multiple paths
pub async fn upload_receipt(
    client: &Client,
    bucket: &str,
    receipt: &ChecksumJobReceipt,
    paths: &[String],
) -> Result<Vec<String>, BatchError> {
    let bytes = Bytes::from(serde_json::to_vec(receipt)?);

    let uploads = paths.iter().map(|path| {
        let file = File::new(bucket, path);
        let stream = ByteStream::from(bytes.clone());
        async move {
            file::upload(client, &file, stream, "application/json").await?;
            Ok::<_, BatchError>(file.http_url())
        }
    });

    try_join_all(uploads).await
}
