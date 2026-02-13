use apputils::{
    stack::{self, DateCtx},
    stats::VerificationStats,
};
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;

use crate::{
    batch::{ChecksumJobReceipt, get_manifest_if_ready},
    checksum,
    config::Config,
    file::{self, File},
};

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

/// Generate a consolidated checksum report using batch compute checksum results
pub async fn perform(
    config: &Config,
    job_file: &File,
    opts: &PerformOptions,
) -> Result<VerificationStats, checksum::ChecksumError> {
    tracing::info!("Retrieving job receipt from S3: {}", job_file.s3_url());

    let bytes = file::download_bytes(config.s3(), job_file).await?;
    let receipt: ChecksumJobReceipt = serde_json::from_slice(&bytes)?;
    let source_bucket = receipt.source_bucket.clone();
    let managed_bucket = config.stack().managed_bucket();

    let Some(source) =
        get_manifest_if_ready(config, &receipt.source_bucket, &receipt.source_job_id).await?
    else {
        tracing::info!("Source job {} not ready yet", receipt.source_job_id);
        return Ok(checksum::empty_stats());
    };

    tracing::info!("Source job file found: {:?}", &source);

    let Some(repl) =
        get_manifest_if_ready(config, &receipt.repl_bucket, &receipt.repl_job_id).await?
    else {
        tracing::info!("Replication job {} not ready yet", receipt.repl_job_id);
        return Ok(checksum::empty_stats());
    };

    tracing::info!("Replication job file found: {:?}", &repl);

    let temp_dir = tempfile::tempdir()?;
    let source_paths = checksum::download_manifest_files(config, source.results, &temp_dir).await?;
    let repl_paths = checksum::download_manifest_files(config, repl.results, &temp_dir).await?;

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
        let csv_path = config.stack().reports_checksums_path(&source_bucket, ctx);
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
            .metadata_checksums_stats_path(&source_bucket, ctx);
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
