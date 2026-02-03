use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3control::{
    self as s3control,
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
    bucket::RequestError,
    config::{BatchConfig, RequestConfig},
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
        .map_err(|e| BatchError::S3Control(Box::new(e)))?;

    Ok(response.job_id.unwrap_or_default())
}

pub async fn get_batch_manifest(
    config: &RequestConfig,
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

    if !file::exists(&config.client, manifest).await {
        tracing::info!("Manifest not found: {}", manifest.s3_url());
        return Err(BatchError::ManifestNotFound(manifest.s3_url()));
    }

    BatchManifest::fetch(&config.client, manifest).await
}

pub async fn get_job_status(config: &BatchConfig, job_id: &str) -> Result<JobStatus, BatchError> {
    let resp = config
        .client
        .describe_job()
        .account_id(config.account_id())
        .job_id(job_id)
        .send()
        .await
        .map_err(|e| BatchError::S3Control(Box::new(e)))?;

    let job = resp
        .job
        .ok_or(BatchError::JobNotFound(job_id.to_string()))?;

    let status = job
        .status
        .ok_or(BatchError::MissingStatus(job_id.to_string()))?;

    Ok(status)
}

pub async fn get_manifest_if_ready(
    batch: &BatchConfig,
    request: &RequestConfig,
    bucket: &str,
    job_id: &str,
) -> Result<Option<BatchManifest>, RequestError> {
    let status = get_job_status(batch, job_id)
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to get job status: {}", e)))?;

    match status {
        JobStatus::Complete => match get_batch_manifest(request, bucket, job_id).await {
            Ok(manifest) => Ok(Some(manifest)),
            Err(e) => Err(RequestError::S3Error(e.to_string())),
        },
        JobStatus::Failed => Err(RequestError::S3Error(format!("job {} failed", job_id))),
        status => {
            tracing::info!("Job {} not in continuable status: {}", job_id, status);
            dbg!(status);
            Ok(None)
        }
    }
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
