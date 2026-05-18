use std::path::PathBuf;
use std::sync::Arc;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::head_object::{HeadObjectError, HeadObjectOutput};
use aws_sdk_s3::operation::put_object_tagging::PutObjectTaggingError;
use aws_sdk_s3::types::{Tag, Tagging};
use aws_smithy_types::error::display::DisplayErrorContext;
use chrono::{DateTime, Utc};
use csv::{ReaderBuilder, WriterBuilder};
use futures::stream::{self, StreamExt};
use tokio::sync::Mutex;

use crate::errors::ArchiveItError;
use crate::inventory::InventoryRow;

const SHA1_METADATA_KEY: &str = "sha1";
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

#[derive(Debug, Clone)]
pub struct ExpirationPolicy {
    pub older_than: DateTime<Utc>,
    /// When set, tag matching S3 objects with this key/value pair.
    pub tag: Option<(String, String)>,
}

enum Outcome {
    MatchedSha1,
    MatchedSize,
    Unmatched(Box<InventoryRow>, String),
    NotFound(Box<InventoryRow>, String),
    Expired(String),
    Errored(String),
    Skipped(String),
}

pub async fn perform(
    s3: &aws_sdk_s3::Client,
    args: &PerformArgs,
) -> Result<AuditStats, ArchiveItError> {
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_path(&args.inventory)?;
    let writer = WriterBuilder::new()
        .has_headers(true)
        .from_path(&args.sync_out)?;
    let writer = Arc::new(Mutex::new(writer));

    let key_prefix = args.key_prefix.clone().unwrap_or_default();
    let expiration_tagging = build_tagging(args.expiration.as_ref())?;
    let concurrency = args.concurrency.max(1);

    let ctx = Arc::new(RowCtx {
        s3: s3.clone(),
        bucket: args.bucket.clone(),
        key_prefix,
        expiration: args.expiration.clone(),
        expiration_tagging,
    });

    let rows = rdr
        .deserialize::<InventoryRow>()
        .collect::<Result<Vec<_>, _>>()?;

    let mut stats = AuditStats::default();
    let writer_for_stream = writer.clone();
    let mut stream = stream::iter(rows.into_iter().map(|row| {
        let ctx = ctx.clone();
        async move { audit_row(ctx, row).await }
    }))
    .buffer_unordered(concurrency);

    while let Some(outcome) = stream.next().await {
        match outcome {
            Outcome::MatchedSha1 => stats.matched_sha1 += 1,
            Outcome::MatchedSize => stats.matched_size += 1,
            Outcome::Unmatched(row, msg) => {
                stats.unmatched += 1;
                tracing::info!("{msg}");
                let mut w = writer_for_stream.lock().await;
                w.serialize(&*row)?;
                w.flush()?;
            }
            Outcome::NotFound(row, msg) => {
                stats.not_found += 1;
                tracing::info!("{msg}");
                let mut w = writer_for_stream.lock().await;
                w.serialize(&*row)?;
                w.flush()?;
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
    }

    writer.lock().await.flush()?;
    Ok(stats)
}

struct RowCtx {
    s3: aws_sdk_s3::Client,
    bucket: String,
    key_prefix: String,
    expiration: Option<ExpirationPolicy>,
    expiration_tagging: Option<Tagging>,
}

async fn audit_row(ctx: Arc<RowCtx>, row: InventoryRow) -> Outcome {
    let expected_size = row.size_bytes;
    let expected_sha1 = row.sha1.clone().unwrap_or_default();
    let store_time = match DateTime::parse_from_rfc3339(&row.store_time) {
        Ok(t) => t.with_timezone(&Utc),
        Err(e) => {
            return Outcome::Skipped(format!(
                "invalid store_time {:?} for {}: {e}",
                row.store_time, row.filename
            ));
        }
    };
    let is_expired = ctx
        .expiration
        .as_ref()
        .is_some_and(|p| store_time < p.older_than);

    let key = format!("{}{}", ctx.key_prefix, row.filename);
    let bucket = ctx.bucket.as_str();

    let head = match ctx.s3.head_object().bucket(bucket).key(&key).send().await {
        Ok(out) => out,
        Err(SdkError::ServiceError(e)) if matches!(e.err(), HeadObjectError::NotFound(_)) => {
            return if is_expired {
                Outcome::Skipped(format!("not found but expired: s3://{bucket}/{key}"))
            } else {
                let msg = format!("not found: s3://{bucket}/{key}");
                Outcome::NotFound(Box::new(row), msg)
            };
        }
        Err(e) => {
            return Outcome::Errored(format!(
                "head error: s3://{bucket}/{key}: {}",
                DisplayErrorContext(&e)
            ));
        }
    };

    if !is_expired {
        return classify_existing(&head, &row, expected_size, &expected_sha1, bucket, &key);
    }

    let Some(tagging) = ctx.expiration_tagging.clone() else {
        return Outcome::Expired(format!("expired (would tag): s3://{bucket}/{key}"));
    };

    match tag_expired(&ctx.s3, bucket, &key, tagging).await {
        Ok(()) => Outcome::Expired(format!("expired (tagged): s3://{bucket}/{key}")),
        Err(e) => Outcome::Errored(format!(
            "error tagging expired s3://{bucket}/{key}: {}",
            DisplayErrorContext(&e)
        )),
    }
}

fn classify_existing(
    head: &HeadObjectOutput,
    row: &InventoryRow,
    expected_size: u64,
    expected_sha1: &str,
    bucket: &str,
    key: &str,
) -> Outcome {
    let existing_sha1 = head
        .metadata
        .as_ref()
        .and_then(|m| m.get(SHA1_METADATA_KEY))
        .map(String::as_str);
    let existing_size = head.content_length.unwrap_or(0).max(0) as u64;
    match existing_sha1 {
        Some(s) if !expected_sha1.is_empty() && s == expected_sha1 => Outcome::MatchedSha1,
        Some(s) => Outcome::Unmatched(
            Box::new(row.clone()),
            format!(
                "unmatched (sha1 differs): s3://{bucket}/{key} \
                 (existing: {s}, expected: {expected_sha1})"
            ),
        ),
        None if existing_size == expected_size => Outcome::MatchedSize,
        None => Outcome::Unmatched(
            Box::new(row.clone()),
            format!(
                "unmatched (no sha1, size differs): s3://{bucket}/{key} \
                 (existing size: {existing_size}, expected: {expected_size})"
            ),
        ),
    }
}

fn build_tagging(policy: Option<&ExpirationPolicy>) -> Result<Option<Tagging>, ArchiveItError> {
    let Some(policy) = policy else {
        return Ok(None);
    };
    let Some((k, v)) = policy.tag.as_ref() else {
        return Ok(None);
    };
    let tag = Tag::builder()
        .key(k)
        .value(v)
        .build()
        .map_err(|e| ArchiveItError::Io(std::io::Error::other(e.to_string())))?;
    let tagging = Tagging::builder()
        .tag_set(tag)
        .build()
        .map_err(|e| ArchiveItError::Io(std::io::Error::other(e.to_string())))?;
    Ok(Some(tagging))
}

async fn tag_expired(
    s3: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    tagging: Tagging,
) -> Result<(), SdkError<PutObjectTaggingError>> {
    s3.put_object_tagging()
        .bucket(bucket)
        .key(key)
        .tagging(tagging)
        .send()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tagging_returns_none_when_policy_absent() {
        let tagging = build_tagging(None).unwrap();
        assert!(tagging.is_none());
    }

    #[test]
    fn build_tagging_returns_none_when_tag_absent() {
        let policy = ExpirationPolicy {
            older_than: Utc::now(),
            tag: None,
        };
        let tagging = build_tagging(Some(&policy)).unwrap();
        assert!(tagging.is_none());
    }

    #[test]
    fn build_tagging_builds_when_tag_present() {
        let policy = ExpirationPolicy {
            older_than: Utc::now(),
            tag: Some(("expired".into(), "true".into())),
        };
        let tagging = build_tagging(Some(&policy)).unwrap().expect("tagging");
        let tags = tagging.tag_set();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].key(), "expired");
        assert_eq!(tags[0].value(), "true");
    }
}
