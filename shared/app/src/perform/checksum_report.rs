use apputils::{
    stack::{self, DateCtx},
    stats::VerificationStats,
};
use aws_sdk_s3::primitives::ByteStream;
use awsutils::{
    batch::{BatchResultEntry, ChecksumJobReceipt},
    checksum,
    file::{self, File},
};
use bytes::Bytes;

use crate::{batch::get_manifest_if_ready, config::Config, perform::errors::ChecksumReportError};

#[derive(Debug, Clone, Copy)]
pub struct PerformOptions {
    pub date_ctx: DateCtx,
}

impl Default for PerformOptions {
    fn default() -> Self {
        Self {
            date_ctx: DateCtx::Today,
        }
    }
}

#[derive(Debug)]
struct ReadyManifests {
    source_results: Vec<BatchResultEntry>,
    replication_results: Vec<BatchResultEntry>,
}

/// Generate a consolidated checksum report using batch compute checksum results
pub async fn perform(
    config: &Config,
    job_file: &File,
    opts: &PerformOptions,
) -> Result<VerificationStats, ChecksumReportError> {
    tracing::info!("Retrieving job receipt from S3: {}", job_file.s3_url());

    let bytes = file::download_bytes(config.s3(), job_file)
        .await
        .map_err(ChecksumReportError::ReceiptDownload)?;
    let receipt: ChecksumJobReceipt =
        serde_json::from_slice(&bytes).map_err(ChecksumReportError::ReceiptParse)?;
    let source_bucket = receipt.source_bucket.clone();

    let Some(ready_manifests) = resolve_ready_manifests(config, &receipt).await? else {
        return Ok(checksum::empty_stats());
    };

    process_and_upload(
        config,
        &source_bucket,
        ready_manifests.source_results,
        ready_manifests.replication_results,
        opts,
    )
    .await
    .map_err(ChecksumReportError::Processing)
}

async fn resolve_ready_manifests(
    config: &Config,
    receipt: &ChecksumJobReceipt,
) -> Result<Option<ReadyManifests>, ChecksumReportError> {
    let Some(source) =
        get_manifest_if_ready(config, &receipt.source_bucket, &receipt.source_job_id)
            .await
            .map_err(ChecksumReportError::BatchStatus)?
    else {
        tracing::info!("Source job {} not ready yet", receipt.source_job_id);
        return Ok(None);
    };

    tracing::info!("Source job file found: {:?}", &source);

    let Some(repl) = get_manifest_if_ready(config, &receipt.repl_bucket, &receipt.repl_job_id)
        .await
        .map_err(ChecksumReportError::BatchStatus)?
    else {
        tracing::info!("Replication job {} not ready yet", receipt.repl_job_id);
        return Ok(None);
    };

    tracing::info!("Replication job file found: {:?}", &repl);
    Ok(Some(ReadyManifests {
        source_results: source.results,
        replication_results: repl.results,
    }))
}

async fn process_and_upload(
    config: &Config,
    source_bucket: &str,
    source_results: Vec<BatchResultEntry>,
    replication_results: Vec<BatchResultEntry>,
    opts: &PerformOptions,
) -> Result<VerificationStats, checksum::ChecksumError> {
    let managed_bucket = config.stack().managed_bucket();
    let temp_dir = tempfile::tempdir()?;
    let source_paths =
        checksum::download_manifest_files(config.s3(), source_results, &temp_dir).await?;
    let repl_paths =
        checksum::download_manifest_files(config.s3(), replication_results, &temp_dir).await?;

    tracing::info!(
        source_files = source_paths.len(),
        replication_files = repl_paths.len(),
        "Processing checksum report files",
    );

    let (csv, stats) =
        tokio::task::spawn_blocking(move || checksum::process(&source_paths, &repl_paths))
            .await
            .expect("spawn_blocking task panicked")?;

    let csv_bytes = Bytes::from(csv);
    let stats_bytes = Bytes::from(serde_json::to_vec(&stats)?);

    for ctx in [stack::DateCtx::Latest, opts.date_ctx] {
        let csv_path = config.stack().reports_checksums_path(source_bucket, ctx);
        let csv_file = File::new(&managed_bucket, csv_path);

        tracing::info!("Uploading checksum report csv: {}", csv_file.s3_url());
        file::upload(
            config.s3(),
            &csv_file,
            ByteStream::from(csv_bytes.clone()),
            "text/csv",
        )
        .await?;

        let stats_path = config
            .stack()
            .metadata_checksums_stats_path(source_bucket, ctx);
        let stats_file = File::new(&managed_bucket, stats_path);

        tracing::info!(
            "Uploading checksum verification stats: {}",
            stats_file.s3_url()
        );
        file::upload(
            config.s3(),
            &stats_file,
            ByteStream::from(stats_bytes.clone()),
            "application/json",
        )
        .await?;
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use aws_smithy_types::body::SdkBody;

    use super::*;
    use crate::{config as app_config, perform::errors::ChecksumReportError};
    use test_support::{TestClientBuilder, recorded_requests};

    fn batch_result(status: &str, bucket: &str, key: &str) -> BatchResultEntry {
        BatchResultEntry {
            task_execution_status: status.to_string(),
            bucket: bucket.to_string(),
            md5_checksum: "dummy-md5".to_string(),
            key: key.to_string(),
        }
    }

    fn uri_has_key(uri: &str, key: &str) -> bool {
        let encoded_upper = key.replace('/', "%2F");
        let encoded_lower = key.replace('/', "%2f");
        uri.contains(key) || uri.contains(&encoded_upper) || uri.contains(&encoded_lower)
    }

    #[tokio::test]
    async fn test_process_and_upload_writes_latest_and_dated_outputs() {
        let source_bucket = "test-stack-private";
        let source_key = "checksum-report-tests/source.csv";
        let repl_key = "checksum-report-tests/repl.csv";

        let source_csv = include_bytes!("../../../../files/checksum-source.csv");
        let repl_csv = include_bytes!("../../../../files/checksum-replication.csv");

        let (sdk_config, replay) = TestClientBuilder::new()
            .success(SdkBody::from(source_csv.to_vec()), None)
            .success(SdkBody::from(repl_csv.to_vec()), None)
            .ok()
            .ok()
            .ok()
            .ok()
            .build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);
        let managed_bucket = config.stack().managed_bucket();
        let opts = PerformOptions::default();

        let stats = process_and_upload(
            &config,
            source_bucket,
            vec![batch_result("succeeded", &managed_bucket, source_key)],
            vec![batch_result("succeeded", &managed_bucket, repl_key)],
            &opts,
        )
        .await
        .expect("process_and_upload should succeed");

        let requests = recorded_requests(&replay);

        assert!(
            requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, source_key))
        );
        assert!(
            requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, repl_key))
        );

        let csv_puts: Vec<_> = requests
            .iter()
            .filter(|r| r.method == "PUT" && r.content_type.as_deref() == Some("text/csv"))
            .collect();
        assert_eq!(csv_puts.len(), 2);

        let latest_csv_key = config
            .stack()
            .reports_checksums_path(source_bucket, DateCtx::Latest);
        assert!(
            csv_puts
                .iter()
                .any(|r| uri_has_key(&r.uri, latest_csv_key.as_str()))
        );
        assert!(
            csv_puts
                .iter()
                .any(|r| !uri_has_key(&r.uri, latest_csv_key.as_str()))
        );
        assert!(csv_puts.iter().all(|r| r.body == csv_puts[0].body));

        let csv = std::str::from_utf8(&csv_puts[0].body).expect("csv body should be utf-8");
        assert!(csv.starts_with(
            "bucket,key,version_id,status,checksum_algorithm,checksum_source,checksum_replication\n"
        ));

        let stats_puts: Vec<_> = requests
            .iter()
            .filter(|r| r.method == "PUT" && r.content_type.as_deref() == Some("application/json"))
            .collect();
        assert_eq!(stats_puts.len(), 2);

        let latest_stats_key = config
            .stack()
            .metadata_checksums_stats_path(source_bucket, DateCtx::Latest);
        assert!(
            stats_puts
                .iter()
                .any(|r| uri_has_key(&r.uri, latest_stats_key.as_str()))
        );
        assert!(
            stats_puts
                .iter()
                .any(|r| !uri_has_key(&r.uri, latest_stats_key.as_str()))
        );
        assert!(stats_puts.iter().all(|r| r.body == stats_puts[0].body));

        let uploaded_stats_json: serde_json::Value = serde_json::from_slice(&stats_puts[0].body)
            .expect("uploaded stats should be valid json");
        let expected_stats_json =
            serde_json::to_value(&stats).expect("stats should serialize to json");
        assert_eq!(uploaded_stats_json, expected_stats_json);
    }

    #[tokio::test]
    async fn test_process_and_upload_skips_non_succeeded_manifest_entries() {
        let source_bucket = "test-stack-private";
        let source_key = "checksum-report-tests/source.csv";
        let repl_key = "checksum-report-tests/repl.csv";
        let skipped_key = "checksum-report-tests/skipped.csv";

        let source_csv = include_bytes!("../../../../files/checksum-source.csv");
        let repl_csv = include_bytes!("../../../../files/checksum-replication.csv");

        let (sdk_config, replay) = TestClientBuilder::new()
            .success(SdkBody::from(source_csv.to_vec()), None)
            .success(SdkBody::from(repl_csv.to_vec()), None)
            .ok()
            .ok()
            .ok()
            .ok()
            .build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);
        let managed_bucket = config.stack().managed_bucket();
        let opts = PerformOptions::default();

        process_and_upload(
            &config,
            source_bucket,
            vec![
                batch_result("failed", &managed_bucket, skipped_key),
                batch_result("succeeded", &managed_bucket, source_key),
            ],
            vec![batch_result("succeeded", &managed_bucket, repl_key)],
            &opts,
        )
        .await
        .expect("process_and_upload should succeed when failed entries are skipped");

        let requests = recorded_requests(&replay);

        assert!(
            requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, source_key))
        );
        assert!(
            requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, repl_key))
        );
        assert!(
            !requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, skipped_key))
        );
    }

    #[tokio::test]
    async fn test_perform_maps_receipt_download_failure() {
        let sdk_config = TestClientBuilder::new()
            .s3_error("NoSuchKey", "not found")
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);
        let job_file = File::new(config.stack().managed_bucket(), "receipts/missing.json");
        let opts = PerformOptions::default();

        let err = perform(&config, &job_file, &opts)
            .await
            .expect_err("perform should fail when receipt download fails");

        assert!(
            matches!(err, ChecksumReportError::ReceiptDownload(_)),
            "expected ReceiptDownload, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_perform_maps_receipt_parse_failure() {
        let sdk_config = TestClientBuilder::new()
            .success("not valid json", None)
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);
        let job_file = File::new(config.stack().managed_bucket(), "receipts/bad.json");
        let opts = PerformOptions::default();

        let err = perform(&config, &job_file, &opts)
            .await
            .expect_err("perform should fail when receipt is invalid json");

        assert!(
            matches!(err, ChecksumReportError::ReceiptParse(_)),
            "expected ReceiptParse, got: {err:?}"
        );
    }
}
