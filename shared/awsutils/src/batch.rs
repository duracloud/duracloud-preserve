use apputils::stack::DateCtx;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3control::{
    self as s3control,
    error::{DisplayErrorContext, ProvideErrorMetadata, SdkError as S3ControlSdkError},
    operation::RequestId,
    types::{
        ComputeObjectChecksumAlgorithm, ComputeObjectChecksumType, GeneratedManifestFormat,
        JobManifestGenerator, JobOperation, JobReport, JobReportFormat, JobReportScope, JobStatus,
        S3ComputeObjectChecksumOperation, S3JobManifestGenerator, S3ManifestOutputLocation,
    },
};
use bytes::Bytes;
use chrono::Utc;
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::{
    bucket::{Bucket, RequestError},
    config::Config,
    file::{self, File},
};

const CHECKSUM_ALGORITHM: ComputeObjectChecksumAlgorithm =
    ComputeObjectChecksumAlgorithm::Crc64Nvme;
const MANIFEST_PREFIX: &str = "batch/manifests";
const REPORT_PREFIX: &str = "batch/reports";

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("Invalid bucket: {0} (must be a standard or public bucket in the stack)")]
    InvalidBucket(String),
    #[error("Job matching id not found: {0}")]
    JobNotFound(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Manifest not found: {0}")]
    ManifestNotFound(String),
    #[error("Job status matching id not found: {0}")]
    MissingStatus(String),
    #[error("{0}")]
    Request(#[from] RequestError),
    #[error("Partial failure: {}", .0.join("; "))]
    PartialFailure(Vec<String>),
    #[error("S3 Control error: {0:#}")]
    S3Control(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Batch Manifest
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BatchManifest {
    pub format: String,
    pub report_creation_date: String,
    pub results: Vec<BatchResultEntry>,
    pub report_schema: String,
}

impl BatchManifest {
    pub async fn fetch(client: &Client, file: &File) -> Result<Self, BatchError> {
        let bytes = file::download_bytes(client, file).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

/// Batch Result Entry
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BatchResultEntry {
    pub task_execution_status: String,
    pub bucket: String,
    #[serde(rename = "MD5Checksum")]
    pub md5_checksum: String,
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChecksumJobReceipt {
    pub source_bucket: String,
    pub source_job_id: String,
    pub repl_bucket: String,
    pub repl_job_id: String,
    pub created_at: String,
}

impl ChecksumJobReceipt {
    pub fn new(
        source_bucket: &str,
        source_job_id: &str,
        repl_bucket: &str,
        repl_job_id: &str,
    ) -> Self {
        Self {
            source_bucket: source_bucket.to_string(),
            source_job_id: source_job_id.to_string(),
            repl_bucket: repl_bucket.to_string(),
            repl_job_id: repl_job_id.to_string(),
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

struct JobParams<'a> {
    client: &'a s3control::Client,
    account_id: &'a str,
    role_arn: &'a str,
    job_type: &'a str,
    description: &'a str,
    operation: JobOperation,
    manifest_generator: JobManifestGenerator,
    report: JobReport,
}

pub async fn create_checksum_job(
    client: &s3control::Client,
    account_id: &str,
    role_arn: &str,
    source_bucket: &str,
    report_bucket: &str,
) -> Result<String, BatchError> {
    let operation = JobOperation::builder()
        .s3_compute_object_checksum(
            S3ComputeObjectChecksumOperation::builder()
                .checksum_algorithm(CHECKSUM_ALGORITHM)
                .checksum_type(ComputeObjectChecksumType::FullObject)
                .build(),
        )
        .build();

    let job_type = "checksum";
    let manifest_generator = build_manifest(job_type, account_id, source_bucket, report_bucket)
        .map_err(BatchError::S3Control)?;
    let report = build_report(job_type, account_id, source_bucket, report_bucket);

    run(JobParams {
        client,
        account_id,
        role_arn,
        job_type,
        description: "Compute object checksums",
        operation,
        manifest_generator,
        report,
    })
    .await
}

fn build_manifest(
    job_type: &str,
    account_id: &str,
    source_bucket: &str,
    report_bucket: &str,
) -> Result<JobManifestGenerator, Box<dyn std::error::Error + Send + Sync>> {
    Ok(JobManifestGenerator::S3JobManifestGenerator(
        S3JobManifestGenerator::builder()
            .expected_bucket_owner(account_id)
            .source_bucket(format!("arn:aws:s3:::{}", source_bucket))
            .enable_manifest_output(true)
            .manifest_output_location(
                S3ManifestOutputLocation::builder()
                    .expected_manifest_bucket_owner(account_id)
                    .bucket(format!("arn:aws:s3:::{}", report_bucket))
                    .manifest_prefix(format!("{}/{}", MANIFEST_PREFIX, job_type))
                    .manifest_format(GeneratedManifestFormat::S3InventoryReportCsv20211130)
                    .build()?,
            )
            .build()?,
    ))
}

fn build_report(
    job_type: &str,
    account_id: &str,
    source_bucket: &str,
    report_bucket: &str,
) -> JobReport {
    JobReport::builder()
        .enabled(true)
        .bucket(format!("arn:aws:s3:::{}", report_bucket))
        .expected_bucket_owner(account_id)
        .prefix(format!("{}/{}/{}", REPORT_PREFIX, job_type, source_bucket))
        .format(JobReportFormat::ReportCsv20180820)
        .report_scope(JobReportScope::AllTasks)
        .build()
}

async fn run(params: JobParams<'_>) -> Result<String, BatchError> {
    let response = params
        .client
        .create_job()
        .account_id(params.account_id)
        .operation(params.operation)
        .manifest_generator(params.manifest_generator)
        .report(params.report)
        .priority(10)
        .role_arn(params.role_arn)
        .client_request_token(format!(
            "{}-job-{}",
            params.job_type,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .description(params.description)
        .confirmation_required(false)
        .send()
        .await
        .map_err(|e| BatchError::S3Control(Box::new(s3control_error("CreateJob", &e))))?;

    response
        .job_id
        .ok_or_else(|| BatchError::S3Control("missing job_id in response".into()))
}

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
        .map_err(|e| BatchError::S3Control(Box::new(s3control_error("DescribeJob", &e))))?;

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

fn s3control_error<E>(operation: &str, e: &S3ControlSdkError<E>) -> std::io::Error
where
    E: std::error::Error + ProvideErrorMetadata + std::fmt::Debug + Send + Sync + 'static,
{
    let code = e.code().unwrap_or("unknown");
    let message = e.message().unwrap_or("unknown");
    let request_id = e.request_id().unwrap_or("unknown");

    std::io::Error::other(format!(
        "{operation} failed: code={code}, message={message}, request_id={request_id}, context={}",
        DisplayErrorContext(e)
    ))
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

    let source_result = create_checksum_job(
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

    let replication_result = match create_checksum_job(
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
