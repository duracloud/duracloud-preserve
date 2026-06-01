use aws_sdk_s3control::types::{JobDescriptor, JobStatus};
use base::stack::DateCtx;
use constants::APPLICATION_JSON;

use awsutils::{
    batch::{self as aws_batch, BatchManifest, ChecksumJobReceipt, ReadyManifests},
    bucket::Bucket,
    file::{self, File},
};

use crate::{config::Config, errors::BatchStatusError, upload};

/// Outcome of resolving a single batch job's generated manifest.
enum ManifestOutcome {
    /// Job completed; its manifest is available to process.
    Ready(BatchManifest),
    /// Job failed only because its generated manifest matched no objects (empty)
    Empty,
    /// Job is not yet in a terminal, processable state.
    NotReady,
}

/// Describe a batch job, returning its full descriptor.
async fn describe_job(config: &Config, job_id: &str) -> Result<JobDescriptor, BatchStatusError> {
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

    resp.job
        .ok_or(BatchStatusError::JobNotFound(job_id.to_string()))
}

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

    if !file::exists(config.s3(), manifest)
        .await
        .map_err(aws_batch::BatchError::from)?
    {
        tracing::info!("Manifest not found: {}", manifest.s3_url());
        return Err(BatchStatusError::ManifestNotFound(manifest.s3_url()));
    }

    BatchManifest::fetch(config.s3(), manifest)
        .await
        .map_err(BatchStatusError::from)
}

/// Resolve a single batch job into a [`ManifestOutcome`].
///
/// An empty manifest is reported as [`ManifestOutcome::Empty`] rather than
/// judged here: whether that is a benign no-op or an error depends on whether it
/// is the source or replication job, which only [`resolve_ready_manifests`] knows.
async fn get_manifest(
    config: &Config,
    bucket: &str,
    job_id: &str,
) -> Result<ManifestOutcome, BatchStatusError> {
    let job = describe_job(config, job_id).await?;

    let status = job
        .status
        .clone()
        .ok_or(BatchStatusError::MissingStatus(job_id.to_string()))?;

    match status {
        JobStatus::Complete => Ok(ManifestOutcome::Ready(
            fetch_manifest(config, bucket, job_id).await?,
        )),
        JobStatus::Failed if is_empty_manifest(&job) => Ok(ManifestOutcome::Empty),
        JobStatus::Failed => {
            tracing::error!("Job {} failed batch processing", job_id);
            Err(BatchStatusError::JobFailed(job_id.to_string()))
        }
        status => {
            tracing::info!("Job {} not in continuable status: {}", job_id, status);
            Ok(ManifestOutcome::NotReady)
        }
    }
}

/// Get a batch job's current status
pub async fn get_job_status(config: &Config, job_id: &str) -> Result<JobStatus, BatchStatusError> {
    let job = describe_job(config, job_id).await?;
    job.status
        .ok_or(BatchStatusError::MissingStatus(job_id.to_string()))
}

/// True when a failed job's only failure is an empty generated manifest.
fn is_empty_manifest(job: &JobDescriptor) -> bool {
    job.failure_reasons()
        .iter()
        .any(|f| f.failure_code() == Some(aws_batch::EMPTY_MANIFEST_CODE))
}

/// Resolve both source and replication manifests for a checksum job receipt.
///
/// The source bucket is the gate. An empty source is a benign no-op (`Ok(None)`).
/// Once the source is known to hold objects, an empty replication bucket is a
/// real divergence and surfaces as [`BatchStatusError::EmptyReplication`].
/// Returns `Ok(None)` while either job is still running.
pub async fn resolve_ready_manifests(
    config: &Config,
    receipt: &ChecksumJobReceipt,
) -> Result<Option<ReadyManifests>, BatchStatusError> {
    let source = match get_manifest(config, &receipt.source_bucket, &receipt.source_job_id).await? {
        ManifestOutcome::Ready(manifest) => manifest,
        ManifestOutcome::Empty => {
            tracing::info!(
                "Source bucket {} is empty; nothing to report",
                receipt.source_bucket
            );
            return Ok(None);
        }
        ManifestOutcome::NotReady => {
            tracing::info!("Source job {} not ready yet", receipt.source_job_id);
            return Ok(None);
        }
    };

    tracing::info!("Source job file found: {:?}", &source);

    let repl = match get_manifest(config, &receipt.repl_bucket, &receipt.repl_job_id).await? {
        ManifestOutcome::Ready(manifest) => manifest,
        ManifestOutcome::Empty => {
            tracing::error!(
                "Replication bucket {} is empty but source {} has objects",
                receipt.repl_bucket,
                receipt.source_bucket
            );
            return Err(BatchStatusError::EmptyReplication(
                receipt.repl_bucket.clone(),
            ));
        }
        ManifestOutcome::NotReady => {
            tracing::info!("Replication job {} not ready yet", receipt.repl_job_id);
            return Ok(None);
        }
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

#[cfg(test)]
mod tests {
    use aws_sdk_s3control::types::{JobDescriptor, JobFailure, JobStatus};
    use awsutils::batch::{ChecksumJobReceipt, EMPTY_MANIFEST_CODE};
    use test_support::{TestClientBuilder, describe_job_xml};

    use super::*;
    use crate::config as app_config;

    /// Minimal valid `BatchManifest` JSON for a completed job's manifest object.
    const MANIFEST_JSON: &str = r#"{"Format":"Report_CSV_20180820","ReportCreationDate":"2026-05-31T08:00:00Z","Results":[],"ReportSchema":"Bucket, Key"}"#;

    fn descriptor(status: JobStatus, failure_codes: &[&str]) -> JobDescriptor {
        let mut builder = JobDescriptor::builder().job_id("job").status(status);
        for code in failure_codes {
            builder = builder.failure_reasons(JobFailure::builder().failure_code(*code).build());
        }
        builder.build()
    }

    fn receipt() -> ChecksumJobReceipt {
        ChecksumJobReceipt::new("src-bucket", "src-job", "repl-bucket", "repl-job")
    }

    // --- is_empty_manifest: failure-code matching

    #[test]
    fn empty_manifest_code_is_recognized() {
        let job = descriptor(JobStatus::Failed, &[EMPTY_MANIFEST_CODE]);
        assert!(is_empty_manifest(&job));
    }

    #[test]
    fn other_failure_codes_are_not_empty_manifest() {
        let job = descriptor(JobStatus::Failed, &["InternalError"]);
        assert!(!is_empty_manifest(&job));
    }

    #[test]
    fn no_failure_reasons_is_not_empty_manifest() {
        let job = descriptor(JobStatus::Failed, &[]);
        assert!(!is_empty_manifest(&job));
    }

    // --- resolve_ready_manifests: source-gated decision

    /// Queue a completed source job plus its manifest fetch (describe + head + get).
    fn with_ready_source(builder: TestClientBuilder, job_id: &str) -> TestClientBuilder {
        builder
            .success(describe_job_xml(job_id, "Complete", None), None)
            .ok() // head_object: manifest exists
            .success(MANIFEST_JSON, None) // get_object: manifest body
    }

    #[tokio::test]
    async fn empty_source_is_a_noop() {
        // Only the source describe is consumed: an empty source short-circuits
        // before the replication bucket is ever examined.
        let sdk_config = TestClientBuilder::new()
            .success(
                describe_job_xml("src-job", "Failed", Some(EMPTY_MANIFEST_CODE)),
                None,
            )
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let result = resolve_ready_manifests(&config, &receipt())
            .await
            .expect("empty source should resolve cleanly");
        assert!(result.is_none(), "empty source should be a no-op");
    }

    #[tokio::test]
    async fn running_source_is_not_ready() {
        let sdk_config = TestClientBuilder::new()
            .success(describe_job_xml("src-job", "Active", None), None)
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let result = resolve_ready_manifests(&config, &receipt())
            .await
            .expect("running source should resolve cleanly");
        assert!(result.is_none(), "running source should not be ready");
    }

    #[tokio::test]
    async fn both_ready_produces_a_report() {
        let mut builder = TestClientBuilder::new();
        builder = with_ready_source(builder, "src-job");
        builder = with_ready_source(builder, "repl-job");
        let config = app_config::Config::for_tests(builder.build_sdk_config(), false);

        let result = resolve_ready_manifests(&config, &receipt())
            .await
            .expect("both ready should resolve");
        assert!(result.is_some(), "both jobs ready should produce a report");
    }

    #[tokio::test]
    async fn populated_source_with_empty_replication_is_a_divergence() {
        let mut builder = TestClientBuilder::new();
        builder = with_ready_source(builder, "src-job");
        builder = builder.success(
            describe_job_xml("repl-job", "Failed", Some(EMPTY_MANIFEST_CODE)),
            None,
        );
        let config = app_config::Config::for_tests(builder.build_sdk_config(), false);

        let err = resolve_ready_manifests(&config, &receipt())
            .await
            .expect_err("empty replication bucket against a populated source must error");
        assert!(
            matches!(&err, BatchStatusError::EmptyReplication(bucket) if bucket == "repl-bucket"),
            "expected EmptyReplication, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn running_replication_is_not_ready() {
        let mut builder = TestClientBuilder::new();
        builder = with_ready_source(builder, "src-job");
        builder = builder.success(describe_job_xml("repl-job", "Active", None), None);
        let config = app_config::Config::for_tests(builder.build_sdk_config(), false);

        let result = resolve_ready_manifests(&config, &receipt())
            .await
            .expect("running replication should resolve cleanly");
        assert!(result.is_none(), "running replication should not be ready");
    }
}
