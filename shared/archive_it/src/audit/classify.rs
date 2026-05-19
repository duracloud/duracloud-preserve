use std::sync::Arc;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::head_object::{HeadObjectError, HeadObjectOutput};
use aws_sdk_s3::types::Tagging;
use aws_smithy_types::error::display::DisplayErrorContext;
use chrono::{DateTime, Utc};

use super::expiration::{ExpirationPolicy, tag_expired};
use crate::inventory::InventoryRow;

const SHA1_METADATA_KEY: &str = "sha1";

/// Outcome of auditing a single inventory row against S3.
///
/// `Unmatched` and `NotFound` carry the row so the orchestrator can write
/// it to the sync CSV; other variants carry only a log message.
pub enum Outcome {
    MatchedSha1,
    MatchedSize,
    Unmatched(Box<InventoryRow>, String),
    NotFound(Box<InventoryRow>, String),
    Expired(String),
    Errored(String),
    Skipped(String),
}

/// Per-row execution context shared across all concurrent audit tasks.
pub struct RowCtx {
    pub s3: aws_sdk_s3::Client,
    pub bucket: String,
    pub key_prefix: String,
    pub expiration: Option<ExpirationPolicy>,
    pub expiration_tagging: Option<Tagging>,
}

pub async fn audit_row(ctx: Arc<RowCtx>, row: InventoryRow) -> Outcome {
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
