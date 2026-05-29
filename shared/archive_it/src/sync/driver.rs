use std::pin::pin;
use std::time::{Duration, Instant};

use archive_it_client::http_ferry::Error;
use archive_it_client::models::wasapi::WasapiFile;
use archive_it_client::{DownloadOutcome, WasapiClient};
use futures::StreamExt;

/// Minimum gap between per-file `Progress` log lines. Keeps tracing output
/// readable for slow WASAPI fetches without going silent for minutes.
const PROGRESS_LOG_INTERVAL: Duration = Duration::from_secs(30);

/// `[N/M]` label prepended to a row's log lines.
#[derive(Debug, Clone, Copy)]
pub struct RowLabel {
    pub n: usize,
    pub total: usize,
}

/// Counters returned by [`sync_one_row`], accumulated into `SyncStats` by
/// the caller. Variants mirror the four log outcomes per row.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RowCounts {
    pub uploaded: u64,
    pub skipped: u64,
    pub wasapi_missing: u64,
    pub failed: u64,
}

/// Drive a single WASAPI → S3 download stream to completion. The function
/// owns the per-row tracing and progress-throttling; the caller just sums
/// the returned counts.
pub async fn sync_one_row(
    client: &WasapiClient,
    s3: &aws_sdk_s3::Client,
    bucket: &str,
    key_prefix: Option<&str>,
    file: WasapiFile,
    label: RowLabel,
) -> RowCounts {
    let mut stream = pin!(client.download_to_s3(
        file,
        s3.clone(),
        bucket.to_string(),
        key_prefix.map(str::to_string),
    ));
    let RowLabel { n, total } = label;
    let mut counts = RowCounts::default();
    let mut last_progress_log = Instant::now();

    while let Some(outcome) = stream.next().await {
        match outcome {
            DownloadOutcome::Downloaded { file, verified, .. } => {
                tracing::info!(
                    filename = %file.filename,
                    verified,
                    "[{n}/{total}] Uploaded"
                );
                counts.uploaded += 1;
            }
            DownloadOutcome::Skipped { file, .. } => {
                tracing::info!(
                    filename = %file.filename,
                    "[{n}/{total}] Skipped (already present in S3)"
                );
                counts.skipped += 1;
            }
            DownloadOutcome::Failed { file, error } => {
                if matches!(error, Error::NotFound(_)) {
                    tracing::warn!(
                        filename = %file.filename,
                        %error,
                        "[{n}/{total}] WASAPI no longer has this file"
                    );
                    counts.wasapi_missing += 1;
                } else {
                    tracing::error!(
                        filename = %file.filename,
                        %error,
                        "[{n}/{total}] Re-sync failed"
                    );
                    counts.failed += 1;
                }
            }
            DownloadOutcome::Progress {
                file,
                received,
                total: bytes_total,
            } => {
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
                        "[{n}/{total}] Download progress: {pct:.1}%"
                    );
                    last_progress_log = Instant::now();
                }
            }
            DownloadOutcome::StreamFailed { error } => {
                tracing::error!(
                    error = %error,
                    "[{n}/{total}] Stream error during re-sync"
                );
                counts.failed += 1;
                break;
            }
        }
    }

    counts
}
