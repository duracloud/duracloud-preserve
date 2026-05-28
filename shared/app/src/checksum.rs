use aws_smithy_types::error::display::DisplayErrorContext;
use awsutils::file::{self, File};
use futures::stream::{self, TryStreamExt};

use crate::{config::Config, errors::ChecksumRequestError};

pub const CHECKSUM_TYPE: &str = "crc64nvme";
const CONCURRENCY: usize = 64;
const HEADERS: [&str; 8] = [
    "bucket",
    "key",
    "size",
    "content_type",
    "checksum",
    "checksum_algorithm",
    "status",
    "detail",
];

const STATUS_OK: &str = "ok";
const STATUS_NOT_FOUND: &str = "not_found";
const STATUS_MISSING_CHECKSUM: &str = "missing_checksum";
const STATUS_ERROR: &str = "error";

#[derive(Default)]
struct ChecksumResult {
    bucket: String,
    key: String,
    size: String,
    content_type: String,
    checksum: String,
    checksum_algorithm: &'static str,
    status: &'static str,
    detail: String,
}

pub struct InventoryRow {
    pub bucket: String,
    pub key: String,
}

pub async fn generate_inventory(
    config: &Config,
    rows: impl Iterator<Item = Result<InventoryRow, ChecksumRequestError>>,
) -> Result<(Vec<u8>, usize, usize), ChecksumRequestError> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record(HEADERS)?;

    let client = config.s3();
    let (wtr, count, skipped) = stream::iter(rows)
        .map_ok(|row| async move {
            let file = File::new(&row.bucket, &row.key);
            let head = match file::head(client, &file).await {
                Ok(head) => head,
                Err(err) => {
                    let is_not_found = err.as_service_error().is_some_and(|e| e.is_not_found());

                    if is_not_found {
                        return Ok::<_, ChecksumRequestError>(ChecksumResult {
                            bucket: row.bucket,
                            key: row.key,
                            status: STATUS_NOT_FOUND,
                            ..Default::default()
                        });
                    }

                    let detail = DisplayErrorContext(err).to_string();
                    tracing::warn!(
                        bucket = row.bucket,
                        key = row.key,
                        %detail,
                        "Head request failed",
                    );

                    return Ok::<_, ChecksumRequestError>(ChecksumResult {
                        bucket: row.bucket,
                        key: row.key,
                        status: STATUS_ERROR,
                        detail,
                        ..Default::default()
                    });
                }
            };

            let (checksum, checksum_algorithm, status) = match head.checksum_crc64_nvme() {
                Some(c) => (c.to_string(), CHECKSUM_TYPE, STATUS_OK),
                None => (String::new(), "", STATUS_MISSING_CHECKSUM),
            };

            let size = head.content_length.unwrap_or(0).to_string();
            let content_type = head.content_type.unwrap_or(String::new());

            Ok(ChecksumResult {
                bucket: row.bucket,
                key: row.key,
                size,
                content_type,
                checksum,
                checksum_algorithm,
                status,
                detail: String::new(),
            })
        })
        .try_buffer_unordered(CONCURRENCY)
        .try_fold(
            (wtr, 0usize, 0usize),
            |(mut wtr, count, skipped), row| async move {
                wtr.write_record([
                    &row.bucket,
                    &row.key,
                    &row.size,
                    &row.content_type,
                    &row.checksum,
                    row.checksum_algorithm,
                    row.status,
                    &row.detail,
                ])?;
                let skipped = if row.status == STATUS_OK {
                    skipped
                } else {
                    skipped + 1
                };
                Ok((wtr, count + 1, skipped))
            },
        )
        .await?;

    let csv_bytes = wtr
        .into_inner()
        .map_err(|e| csv::Error::from(e.into_error()))?;

    Ok((csv_bytes, count, skipped))
}

/// Parse an S3 inventory CSV, yielding the `bucket` and `key` columns as `InventoryRow`s.
pub fn parse_inventory_rows(
    bytes: &[u8],
) -> impl Iterator<Item = Result<InventoryRow, ChecksumRequestError>> + '_ {
    csv::ReaderBuilder::new()
        .from_reader(bytes)
        .into_records()
        .map(|result| {
            let record = result?;
            Ok(InventoryRow {
                bucket: record[0].to_string(),
                key: record[1].to_string(),
            })
        })
}
