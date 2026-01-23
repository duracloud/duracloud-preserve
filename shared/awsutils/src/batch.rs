use aws_sdk_s3control::{
    self as s3control,
    types::{
        ComputeObjectChecksumAlgorithm, ComputeObjectChecksumType, GeneratedManifestFormat,
        JobManifestGenerator, JobOperation, JobReport, JobReportFormat, JobReportScope,
        S3ComputeObjectChecksumOperation, S3JobManifestGenerator, S3ManifestOutputLocation,
    },
};
use std::time::{SystemTime, UNIX_EPOCH};

const MANIFEST_PREFIX: &str = "batch/manifests/";
const REPORT_PREFIX: &str = "batch/reports/";

pub enum BatchError {}

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
                .checksum_algorithm(ComputeObjectChecksumAlgorithm::Crc64Nvme)
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
        .prefix(REPORT_PREFIX)
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
