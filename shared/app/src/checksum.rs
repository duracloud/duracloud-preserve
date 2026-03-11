use awsutils::file::{self, File};
use futures::stream::{self, TryStreamExt};

use crate::{config::Config, errors::ChecksumInventoryError};

const CONCURRENCY: usize = 64;
const HEADERS: [&str; 5] = ["bucket", "key", "checksum", "size", "status"];

const STATUS_OK: &str = "ok";
const STATUS_NOT_FOUND: &str = "not_found";
const STATUS_MISSING_CHECKSUM: &str = "missing_checksum";
const STATUS_ERROR: &str = "error";

struct ChecksumResult {
    bucket: String,
    key: String,
    checksum: String,
    size: String,
    status: &'static str,
}

pub struct InventoryRow {
    pub bucket: String,
    pub key: String,
    pub size: String,
}

pub async fn generate_checksum_inventory(
    config: &Config,
    rows: impl Iterator<Item = Result<InventoryRow, ChecksumInventoryError>>,
) -> Result<(Vec<u8>, usize, usize), ChecksumInventoryError> {
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

                    if !is_not_found {
                        tracing::warn!(
                            bucket = row.bucket,
                            key = row.key,
                            %err,
                            "Head request failed",
                        );
                    }

                    let status = if is_not_found {
                        STATUS_NOT_FOUND
                    } else {
                        STATUS_ERROR
                    };

                    return Ok::<_, ChecksumInventoryError>(ChecksumResult {
                        bucket: row.bucket,
                        key: row.key,
                        checksum: String::new(),
                        size: row.size,
                        status,
                    });
                }
            };

            let (checksum, status) = match head.checksum_crc64_nvme() {
                Some(c) => (c.to_string(), STATUS_OK),
                None => (String::new(), STATUS_MISSING_CHECKSUM),
            };

            Ok(ChecksumResult {
                bucket: row.bucket,
                key: row.key,
                checksum,
                size: row.size,
                status,
            })
        })
        .try_buffer_unordered(CONCURRENCY)
        .try_fold(
            (wtr, 0usize, 0usize),
            |(mut wtr, count, skipped), row| async move {
                wtr.write_record([&row.bucket, &row.key, &row.checksum, &row.size, row.status])?;
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
