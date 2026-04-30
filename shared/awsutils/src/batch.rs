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
use base::{
    ManagedFile, Stack,
    bucket::{Bucket, BucketPair},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use futures::future::BoxFuture;

use crate::file::{self, File};

pub use crate::errors::BatchError;
use crate::errors::RequestError;

const CHECKSUM_ALGORITHM: ComputeObjectChecksumAlgorithm =
    ComputeObjectChecksumAlgorithm::Crc64Nvme;

pub struct BatchConfig<'a> {
    pub client: &'a s3control::Client,
    pub account_id: &'a str,
    pub role_arn: &'a str,
    pub stack: &'a Stack,
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
    config: &'a BatchConfig<'a>,
    job_type: &'a str,
    description: &'a str,
    operation: JobOperation,
    manifest_generator: JobManifestGenerator,
    report: JobReport,
}

#[derive(Debug)]
pub struct ReadyManifests {
    pub source_results: Vec<BatchResultEntry>,
    pub replication_results: Vec<BatchResultEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct S3BatchJobDetail {
    pub service_event_details: S3BatchJobStatusChange,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct S3BatchJobStatusChange {
    pub job_id: String,
    pub job_arn: String,
    pub status: String,
    pub failure_codes: Vec<String>,
    pub status_change_reason: Vec<String>,
}

pub async fn create_checksum_job(
    config: &BatchConfig<'_>,
    source_bucket: &str,
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
    let manifest_prefix = config.stack.batch_manifest_prefix(job_type);
    let report_prefix = config.stack.batch_report_prefix(job_type, source_bucket);
    let manifest_generator = build_manifest(config.account_id, source_bucket, &manifest_prefix)
        .map_err(BatchError::S3Control)?;
    let report = build_report(config.account_id, &report_prefix);

    run(JobParams {
        config,
        job_type,
        description: "Compute object checksums",
        operation,
        manifest_generator,
        report,
    })
    .await
}

pub async fn create_copy_job(
    config: &BatchConfig<'_>,
    source_bucket: &str,
    dest_bucket: &str,
    target_prefix: Option<&str>,
) -> Result<String, BatchError> {
    if source_bucket == dest_bucket {
        return Err(RequestError::ValidationError(
            "Source and destination bucket must not be the same".into(),
        )
        .into());
    }

    let mut copy =
        S3CopyObjectOperation::builder().target_resource(format!("arn:aws:s3:::{}", dest_bucket));
    if let Some(prefix) = target_prefix {
        copy = copy.target_key_prefix(prefix);
    }
    let operation = JobOperation::builder()
        .s3_put_object_copy(copy.build())
        .build();

    let job_type = "copy";
    let manifest_prefix = config.stack.batch_manifest_prefix(job_type);
    let report_prefix = config.stack.batch_report_prefix(job_type, source_bucket);
    let manifest_generator = build_manifest(config.account_id, source_bucket, &manifest_prefix)
        .map_err(BatchError::S3Control)?;
    let report = build_report(config.account_id, &report_prefix);

    run(JobParams {
        config,
        job_type,
        description: "Copy objects to destination bucket",
        operation,
        manifest_generator,
        report,
    })
    .await
}

/// Dispatch a job for each bucket pair, collecting all receipts.
/// Returns `Err` with formatted error messages if any pair fails.
pub async fn dispatch_bucket_pair_jobs<'a, F, E>(
    bucket_pairs: &'a [BucketPair],
    trigger: F,
) -> Result<Vec<String>, Vec<String>>
where
    F: Fn(&'a Bucket, &'a Bucket) -> BoxFuture<'a, Result<Vec<String>, E>>,
    E: std::fmt::Display,
{
    let mut receipts = vec![];
    let mut issues = vec![];

    for BucketPair {
        source,
        replication,
    } in bucket_pairs
    {
        match trigger(source, replication).await {
            Ok(urls) => receipts.extend(urls),
            Err(e) => issues.push(format!("{}: {e}", source.name())),
        }
    }

    if issues.is_empty() {
        Ok(receipts)
    } else {
        Err(issues)
    }
}

pub async fn download_manifest_files(
    client: &aws_sdk_s3::Client,
    results: Vec<BatchResultEntry>,
    temp_dir: &tempfile::TempDir,
) -> Result<Vec<String>, RequestError> {
    let files = results
        .into_iter()
        .filter_map(|entry| {
            if !entry
                .task_execution_status
                .eq_ignore_ascii_case("succeeded")
            {
                tracing::warn!(
                    task_execution_status = %entry.task_execution_status,
                    bucket = %entry.bucket,
                    key = %entry.key,
                    "Skipping batch result with non-succeeded task status",
                );
                return None;
            }

            Some(File::new(entry.bucket, entry.key))
        })
        .collect::<Vec<_>>();

    let local_paths =
        file::download_files_to_temp(client, &files, temp_dir, "batch manifest result").await?;

    Ok(local_paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

fn build_manifest(
    account_id: &str,
    source_bucket: &str,
    manifest: &ManagedFile,
) -> Result<JobManifestGenerator, Box<dyn std::error::Error + Send + Sync>> {
    Ok(JobManifestGenerator::S3JobManifestGenerator(
        S3JobManifestGenerator::builder()
            .expected_bucket_owner(account_id)
            .source_bucket(format!("arn:aws:s3:::{}", source_bucket))
            .enable_manifest_output(true)
            .manifest_output_location(
                S3ManifestOutputLocation::builder()
                    .expected_manifest_bucket_owner(account_id)
                    .bucket(format!("arn:aws:s3:::{}", manifest.bucket()))
                    .manifest_prefix(manifest.key())
                    .manifest_format(GeneratedManifestFormat::S3InventoryReportCsv20211130)
                    .build()?,
            )
            .build()?,
    ))
}

fn build_report(account_id: &str, report: &ManagedFile) -> JobReport {
    JobReport::builder()
        .enabled(true)
        .bucket(format!("arn:aws:s3:::{}", report.bucket()))
        .expected_bucket_owner(account_id)
        .prefix(report.key())
        .format(JobReportFormat::ReportCsv20180820)
        .report_scope(JobReportScope::AllTasks)
        .build()
}

async fn run(params: JobParams<'_>) -> Result<String, BatchError> {
    let response = params
        .config
        .client
        .create_job()
        .account_id(params.config.account_id)
        .operation(params.operation)
        .manifest_generator(params.manifest_generator)
        .report(params.report)
        .priority(10)
        .role_arn(params.config.role_arn)
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::{bucket, errors::RequestError};
    use constants::REPLICATION_SUFFIX;
    use test_support::{mock_sdk_config, recorded_requests, replay_xml_event};

    fn test_config(client: &s3control::Client) -> BatchConfig<'_> {
        // Leak the stack to get a 'static lifetime that outlives the config.
        // This is fine in tests — the allocation is tiny and the process exits.
        let stack: &'static Stack = Box::leak(Box::new(Stack::new("test-stack").unwrap()));
        BatchConfig {
            client,
            account_id: "123456789012",
            role_arn: "arn:aws:iam::123456789012:role/test-batch-role",
            stack,
        }
    }

    #[tokio::test]
    async fn test_create_checksum_job_serializes_checksum_operation_and_prefixes() {
        let (sdk_config, replay) = mock_sdk_config(replay_xml_event(
            200,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<CreateJobResult xmlns="http://awss3control.amazonaws.com/doc/2018-08-20/">
  <JobId>checksum-job-1</JobId>
</CreateJobResult>"#,
        ));
        let client = s3control::Client::new(&sdk_config);
        let config = test_config(&client);

        let job_id = create_checksum_job(&config, "source-bucket")
            .await
            .expect("create_checksum_job should succeed");

        assert_eq!(job_id, "checksum-job-1");

        let requests = recorded_requests(&replay);
        assert_eq!(requests.len(), 1, "expected one CreateJob request");
        let body = String::from_utf8(requests[0].body.clone())
            .expect("request body should be valid utf-8");

        assert!(body.contains("<S3ComputeObjectChecksum>"));
        assert!(body.contains("<ChecksumAlgorithm>CRC64NVME</ChecksumAlgorithm>"));
        assert!(body.contains("<ChecksumType>FULL_OBJECT</ChecksumType>"));
        assert!(body.contains("<ManifestPrefix>batch/manifests/checksum</ManifestPrefix>"));
        assert!(body.contains("<Prefix>batch/reports/checksum/source-bucket</Prefix>"));
        assert!(!body.contains("<S3PutObjectCopy>"));
    }

    #[tokio::test]
    async fn test_create_copy_job_maps_create_job_failures_to_batch_error() {
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(
            400,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
  <Code>BadRequestException</Code>
  <Message>invalid request</Message>
</Error>"#,
        ));
        let client = s3control::Client::new(&sdk_config);
        let config = test_config(&client);

        let err = create_copy_job(&config, "source-bucket", "dest-bucket", None)
            .await
            .expect_err("create_copy_job should fail");

        match err {
            BatchError::S3Control(inner) => {
                assert!(inner.to_string().contains("CreateJob failed"));
            }
            other => panic!("expected S3Control error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_create_copy_job_serializes_copy_operation_and_prefixes() {
        let (sdk_config, replay) = mock_sdk_config(replay_xml_event(
            200,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<CreateJobResult xmlns="http://awss3control.amazonaws.com/doc/2018-08-20/">
  <JobId>copy-job-1</JobId>
</CreateJobResult>"#,
        ));
        let client = s3control::Client::new(&sdk_config);
        let config = test_config(&client);

        let job_id = create_copy_job(&config, "source-bucket", "dest-bucket", None)
            .await
            .expect("create_copy_job should succeed");

        assert_eq!(job_id, "copy-job-1");

        let requests = recorded_requests(&replay);
        assert_eq!(requests.len(), 1, "expected one CreateJob request");
        let body = String::from_utf8(requests[0].body.clone())
            .expect("request body should be valid utf-8");

        assert!(body.contains("<S3PutObjectCopy>"));
        assert!(body.contains("<TargetResource>arn:aws:s3:::dest-bucket</TargetResource>"));
        assert!(body.contains("<ManifestPrefix>batch/manifests/copy</ManifestPrefix>"));
        assert!(body.contains("<Prefix>batch/reports/copy/source-bucket</Prefix>"));
        assert!(!body.contains("<TargetKeyPrefix>"));
        assert!(!body.contains("<S3ComputeObjectChecksum>"));
    }

    #[tokio::test]
    async fn test_create_copy_job_includes_target_prefix_when_provided() {
        let (sdk_config, replay) = mock_sdk_config(replay_xml_event(
            200,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<CreateJobResult xmlns="http://awss3control.amazonaws.com/doc/2018-08-20/">
  <JobId>copy-job-2</JobId>
</CreateJobResult>"#,
        ));
        let client = s3control::Client::new(&sdk_config);
        let config = test_config(&client);

        create_copy_job(
            &config,
            "source-bucket",
            "dest-bucket",
            Some("dest-prefix/"),
        )
        .await
        .expect("create_copy_job should succeed");

        let requests = recorded_requests(&replay);
        let body = String::from_utf8(requests[0].body.clone())
            .expect("request body should be valid utf-8");

        assert!(body.contains("<TargetKeyPrefix>dest-prefix/</TargetKeyPrefix>"));
    }

    fn bucket_pair(name: &str, bucket_type: bucket::Type) -> BucketPair {
        let source = Bucket::new(name, bucket_type).expect("source should be valid");
        let replication = Bucket::new(
            format!("{name}{REPLICATION_SUFFIX}").as_str(),
            bucket::Type::Replication,
        )
        .expect("replication should be valid");
        BucketPair::new(source, replication)
    }

    #[tokio::test]
    async fn test_dispatch_bucket_pair_jobs_aggregates_partial_failures() {
        let bucket_pairs = vec![
            bucket_pair("test-stack-alpha", bucket::Type::Standard),
            bucket_pair("test-stack-bravo-public", bucket::Type::Public),
            bucket_pair("test-stack-charlie", bucket::Type::Standard),
        ];

        let issues = dispatch_bucket_pair_jobs(&bucket_pairs, |source, _repl| {
            let source_name = source.name().to_string();
            Box::pin(async move {
                if source_name == "test-stack-bravo-public" || source_name == "test-stack-charlie" {
                    Err(BatchError::Request(RequestError::ValidationError(
                        source_name,
                    )))
                } else {
                    Ok(vec![format!("https://example.local/{source_name}/latest")])
                }
            })
        })
        .await
        .expect_err("dispatch should return partial failure");

        assert_eq!(issues.len(), 2);
        assert!(issues.iter().any(|m| m.contains("test-stack-bravo-public")));
        assert!(issues.iter().any(|m| m.contains("test-stack-charlie")));
    }

    #[tokio::test]
    async fn test_dispatch_bucket_pair_jobs_does_not_short_circuit_after_failure() {
        let bucket_pairs = vec![
            bucket_pair("test-stack-alpha", bucket::Type::Standard),
            bucket_pair("test-stack-bravo-public", bucket::Type::Public),
            bucket_pair("test-stack-charlie", bucket::Type::Standard),
        ];
        let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let issues = dispatch_bucket_pair_jobs(&bucket_pairs, {
            let calls = Arc::clone(&calls);
            move |source, _repl| {
                let calls = Arc::clone(&calls);
                let source_name = source.name().to_string();
                Box::pin(async move {
                    calls.lock().unwrap().push(source_name.clone());
                    if source_name == "test-stack-alpha" {
                        Err(BatchError::Request(RequestError::ValidationError(
                            source_name,
                        )))
                    } else {
                        Ok(vec![format!("https://example.local/{source_name}/latest")])
                    }
                })
            }
        })
        .await
        .expect_err("dispatch should fail due to first bucket");

        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("test-stack-alpha"));

        let seen = calls.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![
                "test-stack-alpha".to_string(),
                "test-stack-bravo-public".to_string(),
                "test-stack-charlie".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_dispatch_bucket_pair_jobs_flattens_receipts_in_pair_order() {
        let bucket_pairs = vec![
            bucket_pair("test-stack-alpha", bucket::Type::Standard),
            bucket_pair("test-stack-bravo-public", bucket::Type::Public),
        ];

        let receipts = dispatch_bucket_pair_jobs(&bucket_pairs, |source, _repl| {
            let source_name = source.name().to_string();
            Box::pin(async move {
                Ok::<_, BatchError>(vec![
                    format!("https://example.local/{source_name}/latest"),
                    format!("https://example.local/{source_name}/today"),
                ])
            })
        })
        .await
        .expect("dispatch should succeed");

        assert_eq!(
            receipts,
            vec![
                "https://example.local/test-stack-alpha/latest".to_string(),
                "https://example.local/test-stack-alpha/today".to_string(),
                "https://example.local/test-stack-bravo-public/latest".to_string(),
                "https://example.local/test-stack-bravo-public/today".to_string(),
            ]
        );
    }
}
