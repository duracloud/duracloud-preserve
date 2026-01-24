use aws_sdk_s3control::{
    self as s3control,
    types::{
        ComputeObjectChecksumAlgorithm, ComputeObjectChecksumType, GeneratedManifestFormat,
        JobManifestGenerator, JobOperation, JobReport, JobReportFormat, JobReportScope,
        S3ComputeObjectChecksumOperation, S3JobManifestGenerator, S3ManifestOutputLocation,
    },
};
use chrono::Utc;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::bucket::RequestError;

const MANIFEST_PREFIX: &str = "batch/manifests";
const REPORT_PREFIX: &str = "batch/reports";

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Request(#[from] RequestError),
    #[error("S3 Control error: {0:#}")]
    S3Control(#[source] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Serialize)]
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

pub async fn create_checksum_job(
    client: &s3control::Client,
    account_id: &str,
    role_arn: &str,
    source_bucket: &str,
    report_bucket: &str,
) -> Result<String, s3control::Error> {
    let operation = JobOperation::builder()
        .s3_compute_object_checksum(
            S3ComputeObjectChecksumOperation::builder()
                .checksum_algorithm(ComputeObjectChecksumAlgorithm::Sha256)
                .checksum_type(ComputeObjectChecksumType::FullObject)
                .build(),
        )
        .build();

    let manifest_generator = JobManifestGenerator::S3JobManifestGenerator(
        S3JobManifestGenerator::builder()
            .expected_bucket_owner(account_id)
            .source_bucket(format!("arn:aws:s3:::{}", source_bucket))
            .enable_manifest_output(true)
            .manifest_output_location(
                S3ManifestOutputLocation::builder()
                    .expected_manifest_bucket_owner(account_id)
                    .bucket(format!("arn:aws:s3:::{}", report_bucket))
                    .manifest_prefix(MANIFEST_PREFIX)
                    .manifest_format(GeneratedManifestFormat::S3InventoryReportCsv20211130)
                    .build()?,
            )
            .build()?,
    );

    let report = JobReport::builder()
        .enabled(true)
        .bucket(format!("arn:aws:s3:::{}", report_bucket))
        .expected_bucket_owner(account_id)
        .prefix(format!("{}/{}", REPORT_PREFIX, source_bucket))
        .format(JobReportFormat::ReportCsv20180820)
        .report_scope(JobReportScope::AllTasks)
        .build();

    let response = client
        .create_job()
        .account_id(account_id)
        .operation(operation)
        .manifest_generator(manifest_generator)
        .report(report)
        .priority(10)
        .role_arn(role_arn)
        .client_request_token(format!(
            "checksum-job-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .description("Compute object checksums")
        .confirmation_required(false)
        .send()
        .await?;

    Ok(response.job_id.unwrap_or_default())
}
