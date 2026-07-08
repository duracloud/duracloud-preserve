use std::path::PathBuf;

use archive_it_client::models::wasapi::WasapiFile;
use archive_it_client::{Config, WasapiClient};
use csv::ReaderBuilder;

use crate::errors::ArchiveItError;
use crate::inventory::InventoryRow;
use crate::sync::{RowLabel, count_data_rows, sync_one_row};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncStats {
    pub uploaded: u64,
    pub skipped: u64,
    pub wasapi_missing: u64,
    pub failed: u64,
}

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub username: String,
    pub password: String,
    /// Optional allow-list header sent on every Archive-It request.
    pub header: Option<(String, String)>,
    /// Local input csv (downloaded from `stack.archive_it_sync(...)`).
    pub sync_in: PathBuf,
    /// S3 bucket sync uploads land in (typically `stack.archive_it_bucket()`).
    pub bucket: String,
    pub key_prefix: Option<String>,
    /// When true, log each row and skip the actual WASAPI download / S3 upload.
    pub dry_run: bool,
}

pub async fn perform(
    s3: &aws_sdk_s3::Client,
    args: &PerformArgs,
) -> Result<SyncStats, ArchiveItError> {
    let client = WasapiClient::with_config(
        args.username.clone(),
        args.password.clone(),
        super::with_header(Config::wasapi(), args.header.as_ref()),
    )?;
    let total = count_data_rows(&args.sync_in)?;
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_path(&args.sync_in)?;

    let mut stats = SyncStats::default();
    for (idx, row) in rdr.deserialize::<InventoryRow>().enumerate() {
        let row = row?;
        let n = idx + 1;
        let file: WasapiFile = (&row).into();
        tracing::info!(
            bucket = %args.bucket,
            filename = %file.filename,
            size_bytes = file.size,
            sha1 = ?file.checksums.sha1,
            "[{n}/{total}] Re-syncing WARC"
        );

        if args.dry_run {
            tracing::info!(
                filename = %file.filename,
                "[{n}/{total}] Dry run; skipping download"
            );
            continue;
        }

        let counts = sync_one_row(
            &client,
            s3,
            &args.bucket,
            args.key_prefix.as_deref(),
            file,
            RowLabel { n, total },
        )
        .await;
        stats.uploaded += counts.uploaded;
        stats.skipped += counts.skipped;
        stats.wasapi_missing += counts.wasapi_missing;
        stats.failed += counts.failed;
    }

    Ok(stats)
}
