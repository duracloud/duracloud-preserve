use std::path::PathBuf;
use std::sync::Arc;

use csv::{ReaderBuilder, WriterBuilder};
use futures::stream::{self, StreamExt};
use tokio::sync::Mutex;

use crate::audit::{ExpirationPolicy, Outcome, RowCtx, audit_row, build_tagging};
use crate::errors::ArchiveItError;
use crate::inventory::InventoryRow;

pub const DEFAULT_CONCURRENCY: usize = 200;

#[derive(Debug, Clone)]
pub struct PerformArgs {
    /// Local input csv (downloaded from `stack.archive_it_inventory(None)`).
    pub inventory: PathBuf,
    /// Local output csv (uploaded to `stack.archive_it_sync(...)`).
    pub sync_out: PathBuf,
    /// S3 bucket to audit against (typically `stack.archive_it_bucket()`).
    pub bucket: String,
    pub key_prefix: Option<String>,
    pub concurrency: usize,
    pub expiration: Option<ExpirationPolicy>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditStats {
    pub matched_sha1: u64,
    pub matched_size: u64,
    pub unmatched: u64,
    pub not_found: u64,
    pub expired: u64,
    pub errored: u64,
    pub skipped: u64,
}

pub async fn perform(
    s3: &aws_sdk_s3::Client,
    args: &PerformArgs,
) -> Result<AuditStats, ArchiveItError> {
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_path(&args.inventory)?;
    let rows = rdr
        .deserialize::<InventoryRow>()
        .collect::<Result<Vec<_>, _>>()?;

    let writer = Arc::new(Mutex::new(
        WriterBuilder::new()
            .has_headers(true)
            .from_path(&args.sync_out)?,
    ));

    let ctx = Arc::new(RowCtx {
        s3: s3.clone(),
        bucket: args.bucket.clone(),
        key_prefix: args.key_prefix.clone().unwrap_or_default(),
        expiration: args.expiration.clone(),
        expiration_tagging: build_tagging(args.expiration.as_ref())?,
    });

    let concurrency = args.concurrency.max(1);
    let mut stream = stream::iter(rows.into_iter().map(|row| {
        let ctx = ctx.clone();
        async move { audit_row(ctx, row).await }
    }))
    .buffer_unordered(concurrency);

    let mut stats = AuditStats::default();
    while let Some(outcome) = stream.next().await {
        record_outcome(&mut stats, outcome, &writer).await?;
    }

    writer.lock().await.flush()?;
    Ok(stats)
}

async fn record_outcome(
    stats: &mut AuditStats,
    outcome: Outcome,
    writer: &Arc<Mutex<csv::Writer<std::fs::File>>>,
) -> Result<(), ArchiveItError> {
    match outcome {
        Outcome::MatchedSha1 => stats.matched_sha1 += 1,
        Outcome::MatchedSize => stats.matched_size += 1,
        Outcome::Unmatched(row, msg) => {
            stats.unmatched += 1;
            tracing::info!("{msg}");
            write_sync_row(writer, &row).await?;
        }
        Outcome::NotFound(row, msg) => {
            stats.not_found += 1;
            tracing::info!("{msg}");
            write_sync_row(writer, &row).await?;
        }
        Outcome::Expired(msg) => {
            stats.expired += 1;
            tracing::info!("{msg}");
        }
        Outcome::Errored(msg) => {
            stats.errored += 1;
            tracing::error!("{msg}");
        }
        Outcome::Skipped(msg) => {
            stats.skipped += 1;
            tracing::warn!("{msg}");
        }
    }
    Ok(())
}

async fn write_sync_row(
    writer: &Arc<Mutex<csv::Writer<std::fs::File>>>,
    row: &InventoryRow,
) -> Result<(), ArchiveItError> {
    let mut w = writer.lock().await;
    w.serialize(row)?;
    w.flush()?;
    Ok(())
}
