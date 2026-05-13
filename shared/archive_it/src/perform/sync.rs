use std::path::{Path, PathBuf};
use std::pin::pin;
use std::process::Command;
use std::time::{Duration, Instant};

use archive_it_client::models::wasapi::WasapiFile;
use archive_it_client::{DownloadOutcome, Error, WasapiClient};
use csv::ReaderBuilder;
use futures::TryStreamExt;

use crate::errors::ArchiveItError;
use crate::inventory::InventoryRow;

/// Minimum gap between per-file `Progress` log lines. Keeps tracing output
/// readable for slow WASAPI fetches without going silent for minutes.
const PROGRESS_LOG_INTERVAL: Duration = Duration::from_secs(30);

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
    /// Local input csv (downloaded from `stack.archive_it_sync()`).
    pub sync_in: PathBuf,
    /// S3 bucket sync uploads land in (typically `stack.archive_it_bucket()`).
    pub bucket: String,
    pub key_prefix: Option<String>,
}

pub async fn perform(
    s3: &aws_sdk_s3::Client,
    args: &PerformArgs,
) -> Result<SyncStats, ArchiveItError> {
    let client = WasapiClient::new(args.username.clone(), args.password.clone())?;
    let total_rows = count_data_rows(&args.sync_in)?;
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
            "[{n}/{total_rows}] Re-syncing WARC"
        );

        let mut stream = pin!(client.download_to_s3(
            file,
            s3.clone(),
            args.bucket.clone(),
            args.key_prefix.clone(),
        ));
        let mut last_progress_log = Instant::now();

        loop {
            match stream.try_next().await {
                Ok(Some(DownloadOutcome::Downloaded { file, verified, .. })) => {
                    tracing::info!(
                        filename = %file.filename,
                        verified,
                        "[{n}/{total_rows}] Uploaded"
                    );
                    stats.uploaded += 1;
                }
                Ok(Some(DownloadOutcome::Skipped { file, .. })) => {
                    tracing::info!(
                        filename = %file.filename,
                        "[{n}/{total_rows}] Skipped (already present in S3)"
                    );
                    stats.skipped += 1;
                }
                Ok(Some(DownloadOutcome::Failed { file, error })) => {
                    if matches!(error, Error::NotFound(_)) {
                        tracing::warn!(
                            filename = %file.filename,
                            %error,
                            "[{n}/{total_rows}] WASAPI no longer has this file"
                        );
                        stats.wasapi_missing += 1;
                    } else {
                        tracing::error!(
                            filename = %file.filename,
                            %error,
                            "[{n}/{total_rows}] Re-sync failed"
                        );
                        stats.failed += 1;
                    }
                }
                Ok(Some(DownloadOutcome::Progress {
                    file,
                    received,
                    total: bytes_total,
                })) => {
                    if last_progress_log.elapsed() >= PROGRESS_LOG_INTERVAL {
                        let pct = if bytes_total == 0 {
                            100.0
                        } else {
                            (received as f64 / bytes_total as f64) * 100.0
                        };
                        tracing::info!(
                            filename = %file.filename,
                            received,
                            bytes_total,
                            "[{n}/{total_rows}] Download progress: {pct:.1}%"
                        );
                        last_progress_log = Instant::now();
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    // Stream-level fault — count once and move to the next row.
                    tracing::error!(
                        error = %e,
                        "[{n}/{total_rows}] Stream error during re-sync"
                    );
                    stats.failed += 1;
                    break;
                }
            }
        }
    }

    tracing::info!(
        uploaded = stats.uploaded,
        skipped = stats.skipped,
        wasapi_missing = stats.wasapi_missing,
        failed = stats.failed,
        "Archive-It sync complete"
    );
    Ok(stats)
}

/// Approximate data-row count via `wc -l` (file lines minus header). Used only
/// for the `[N/M]` log prefix; an embedded newline in a quoted field would
/// undercount but never affects the rows the CSV reader actually processes
/// and is very unlikely for this dataset.
fn count_data_rows(path: &Path) -> Result<usize, ArchiveItError> {
    let output = Command::new("wc").arg("-l").arg(path).output()?;
    if !output.status.success() {
        return Err(ArchiveItError::Io(std::io::Error::other(format!(
            "wc -l failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))));
    }
    let count = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    Ok(count.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use csv::WriterBuilder;

    fn fixture_row(filename: &str) -> InventoryRow {
        InventoryRow {
            collection_id: 1,
            collection_name: "Test".into(),
            account_id: 1,
            filename: filename.into(),
            filetype: "warc".into(),
            size_bytes: 0,
            crawl_id: None,
            crawl_time: None,
            crawl_start: None,
            store_time: "2025-01-01T00:00:00Z".into(),
            sha1: None,
            md5: None,
            primary_location: String::new(),
            all_locations: String::new(),
        }
    }

    #[test]
    fn count_data_rows_empty_file_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.csv");
        std::fs::write(&path, "").unwrap();
        assert_eq!(count_data_rows(&path).unwrap(), 0);
    }

    #[test]
    fn count_data_rows_header_only_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("header.csv");
        std::fs::write(&path, "col_a,col_b\n").unwrap();
        assert_eq!(count_data_rows(&path).unwrap(), 0);
    }

    #[test]
    fn count_data_rows_matches_csv_writer_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.csv");
        let mut wtr = WriterBuilder::new()
            .has_headers(true)
            .from_path(&path)
            .unwrap();
        for filename in ["a.warc.gz", "b.warc.gz", "c.warc.gz"] {
            wtr.serialize(fixture_row(filename)).unwrap();
        }
        wtr.flush().unwrap();
        assert_eq!(count_data_rows(&path).unwrap(), 3);
    }
}
