use apputils::stack::DateCtx;
use aws_sdk_s3control::types::JobStatus;
use constants::APPLICATION_JSON;

use awsutils::{
    batch::{self as aws_batch, BatchManifest, ChecksumJobReceipt, ReadyManifests},
    bucket::Bucket,
    file::{self, File},
};

use crate::{config::Config, errors::BatchStatusError, upload};

/// Download the manifest object for a completed job.
async fn fetch_manifest(
    config: &Config,
    bucket: &str,
    job_id: &str,
) -> Result<BatchManifest, BatchStatusError> {
    let manifest = &File::from(
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

/// Download a completed batch job manifest if it is available.
pub async fn get_manifest(
    config: &Config,
    bucket: &str,
    job_id: &str,
) -> Result<Option<BatchManifest>, BatchStatusError> {
    let status = get_job_status(config, job_id).await?;

    match status {
        JobStatus::Complete => fetch_manifest(config, bucket, job_id).await.map(Some),
        JobStatus::Failed => Err(BatchStatusError::JobFailed(job_id.to_string())),
        status => {
            tracing::info!("Job {} not in continuable status: {}", job_id, status);
            Ok(None)
        }
    }
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

/// Resolve both source and replication manifests for a checksum job receipt.
/// Returns `None` if either job is not yet complete.
pub async fn resolve_ready_manifests(
    config: &Config,
    receipt: &ChecksumJobReceipt,
) -> Result<Option<ReadyManifests>, BatchStatusError> {
    let Some(source) = get_manifest(config, &receipt.source_bucket, &receipt.source_job_id).await?
    else {
        tracing::info!("Source job {} not ready yet", receipt.source_job_id);
        return Ok(None);
    };

    tracing::info!("Source job file found: {:?}", &source);

    let Some(repl) = get_manifest(config, &receipt.repl_bucket, &receipt.repl_job_id).await? else {
        tracing::info!("Replication job {} not ready yet", receipt.repl_job_id);
        return Ok(None);
    };

    tracing::info!("Replication job file found: {:?}", &repl);
    Ok(Some(ReadyManifests {
        source_results: source.results,
        replication_results: repl.results,
    }))
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

    let batch_config = aws_batch::BatchConfig {
        client: config.s3control(),
        account_id: config.account_id(),
        role_arn: config.batch_role_arn(),
        stack: config.stack(),
    };

    let source_result = aws_batch::create_checksum_job(&batch_config, source.name()).await?;

    tracing::info!(
        "Created source checksum job: bucket={}, job_id={}",
        source.name(),
        source_result
    );

    let replication_result =
        match aws_batch::create_checksum_job(&batch_config, replication.name()).await {
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
    let files = [
        stack.metadata_checksums_receipts_path(&source_result, DateCtx::Latest),
        stack.metadata_checksums_receipts_path(&replication_result, DateCtx::Latest),
        stack.metadata_checksums_receipts_path(source.name(), DateCtx::Latest),
        stack.metadata_checksums_receipts_path(source.name(), DateCtx::Today),
    ]
    .map(File::from);

    tracing::info!("Uploading receipt: {:?}", receipt);

    Ok(upload::put_bytes(
        config.s3(),
        serde_json::to_vec(&receipt)?,
        APPLICATION_JSON,
        files,
    )
    .await?)
}
