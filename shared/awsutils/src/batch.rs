use aws_sdk_s3::Client;
use aws_sdk_s3control::{
    self as s3control,
    error::{DisplayErrorContext, ProvideErrorMetadata, SdkError as S3ControlSdkError},
    operation::RequestId,
    types::{
        ComputeObjectChecksumAlgorithm, ComputeObjectChecksumType, GeneratedManifestFormat,
        JobManifestGenerator, JobOperation, JobReport, JobReportFormat, JobReportScope,
        S3ComputeObjectChecksumOperation, S3CopyObjectOperation, S3JobManifestGenerator,
        S3ManifestOutputLocation,
    },
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::{
    bucket::RequestError,
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

pub async fn create_copy_job(
    client: &s3control::Client,
    account_id: &str,
    role_arn: &str,
    source_bucket: &str,
    dest_bucket: &str,
    report_bucket: &str,
) -> Result<String, BatchError> {
    let operation = JobOperation::builder()
        .s3_put_object_copy(
            S3CopyObjectOperation::builder()
                .target_resource(format!("arn:aws:s3:::{}", dest_bucket))
                .build(),
        )
        .build();

    let job_type = "copy";
    let manifest_generator = build_manifest(job_type, account_id, source_bucket, report_bucket)
        .map_err(BatchError::S3Control)?;
    let report = build_report(job_type, account_id, source_bucket, report_bucket);

    run(JobParams {
        client,
        account_id,
        role_arn,
        job_type,
        description: "Copy objects to destination bucket",
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

pub fn s3control_error<E>(operation: &str, e: &S3ControlSdkError<E>) -> std::io::Error
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
