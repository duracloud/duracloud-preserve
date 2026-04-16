use awsutils::{
    batch::{ChecksumJobReceipt, download_manifest_files},
    file::{self, File},
};
use base::{checksum, stack::DateCtx, stats::VerificationStats};
use bytes::Bytes;
use constants::{APPLICATION_JSON, TEXT_CSV};

use crate::{batch, config::Config, errors::ChecksumReportError, upload};

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub job_file: File,
    pub date_ctx: DateCtx,
}

impl PerformArgs {
    pub fn new(job_file: File) -> Self {
        Self {
            job_file,
            date_ctx: DateCtx::Today,
        }
    }
}

/// Generate a consolidated checksum report using batch compute checksum results
pub async fn perform(
    config: &Config,
    args: &PerformArgs,
) -> Result<VerificationStats, ChecksumReportError> {
    let job_file = &args.job_file;
    tracing::info!("Retrieving job receipt from S3: {}", job_file.s3_url());

    let bytes = file::download_bytes(config.s3(), job_file).await?;
    let receipt: ChecksumJobReceipt = serde_json::from_slice(&bytes)?;
    let source_bucket = receipt.source_bucket.clone();

    let Some(ready) = batch::resolve_ready_manifests(config, &receipt).await? else {
        return Ok(VerificationStats::default());
    };

    let temp_dir = tempfile::tempdir()?;
    let source_paths =
        download_manifest_files(config.s3(), ready.source_results, &temp_dir).await?;
    let repl_paths =
        download_manifest_files(config.s3(), ready.replication_results, &temp_dir).await?;

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

    upload::put_versioned_bytes(
        config,
        args.date_ctx,
        csv_bytes,
        TEXT_CSV,
        |ctx| config.stack().reports_checksums_path(&source_bucket, ctx),
        ChecksumReportError::Upload,
    )
    .await?;

    upload::put_versioned_bytes(
        config,
        args.date_ctx,
        stats_bytes,
        APPLICATION_JSON,
        |ctx| {
            config
                .stack()
                .metadata_checksums_stats_path(&source_bucket, ctx)
        },
        ChecksumReportError::Upload,
    )
    .await?;

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config as app_config, errors::ChecksumReportError};
    use test_support::TestClientBuilder;

    #[tokio::test]
    async fn test_perform_maps_receipt_download_failure() {
        let sdk_config = TestClientBuilder::new()
            .s3_error("NoSuchKey", "not found")
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);
        let job_file = File::new(config.stack().managed_bucket(), "receipts/missing.json");
        let args = PerformArgs::new(job_file);

        let err = perform(&config, &args)
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
        let args = PerformArgs::new(job_file);

        let err = perform(&config, &args)
            .await
            .expect_err("perform should fail when receipt is invalid json");

        assert!(
            matches!(err, ChecksumReportError::ReceiptParse(_)),
            "expected ReceiptParse, got: {err:?}"
        );
    }
}
