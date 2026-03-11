use apputils::{content_type::TEXT_CSV, stack::DateCtx};
use aws_sdk_s3::primitives::ByteStream;
use awsutils::file::{self, File};
use bytes::Bytes;

use crate::{
    bucket::bucket_from_csv_key,
    checksum::{InventoryRow, generate_checksum_inventory},
    config::Config,
    errors::ChecksumInventoryError,
};

const CHECKSUM_TYPE: &str = "crc64nvme";

#[derive(Debug, Clone, Copy, Default)]
pub struct PerformOptions {}

pub async fn perform(
    config: &Config,
    csv_file: &File,
    opts: &PerformOptions,
) -> Result<String, ChecksumInventoryError> {
    let _ = opts;
    let bucket = bucket_from_csv_key(csv_file)?;

    let bytes = file::download_bytes(config.s3(), csv_file)
        .await
        .map_err(ChecksumInventoryError::Download)?;

    let mut rdr = csv::ReaderBuilder::new().from_reader(&bytes[..]);

    let rows = rdr
        .records()
        .map(|result| -> Result<InventoryRow, ChecksumInventoryError> {
            let record = result?;
            Ok(InventoryRow {
                bucket: record[0].to_string(),
                key: record[1].to_string(),
                size: record[2].to_string(),
            })
        });

    let (csv_bytes, count, skipped) = generate_checksum_inventory(config, rows).await?;

    tracing::info!("Processed {count} inventory rows");

    if skipped > 0 {
        tracing::warn!("{skipped} of {count} objects had non-ok status");
    }

    let managed_bucket = config.stack().managed_bucket();
    let upload_path = config
        .stack()
        .reports_checksums_path(&format!("{bucket}_{CHECKSUM_TYPE}"), DateCtx::Latest);
    let upload_file = File::new(&managed_bucket, upload_path);

    tracing::info!("Uploading checksum inventory: {}", upload_file.s3_url());

    file::upload(
        config.s3(),
        &upload_file,
        ByteStream::from(Bytes::from(csv_bytes)),
        TEXT_CSV,
    )
    .await
    .map_err(ChecksumInventoryError::Upload)?;

    Ok(upload_file.s3_url())
}

#[cfg(test)]
mod tests {
    use aws_sdk_s3::primitives::SdkBody;
    use test_support::TestClientBuilder;

    use super::*;
    use crate::config as app_config;

    const TEST_CHECKSUM: &str = "BoJQOEj27t0=";
    const CHECKSUM_HEADER: (&str, &str) = ("x-amz-checksum-crc64nvme", TEST_CHECKSUM);

    fn csv_file(config: &Config) -> File {
        File::new(
            config.stack().managed_bucket(),
            "reports/latest/manifests/test-stack-private.csv",
        )
    }

    fn inventory_csv(rows: &[(&str, &str, &str)]) -> String {
        let mut csv =
            "bucket,key,size,last_modified_date,storage_class,replication_status,url\n".to_string();
        for (bucket, key, size) in rows {
            csv.push_str(&format!(
                "{bucket},{key},{size},2026-02-07T04:13:34Z,GLACIER_IR,COMPLETED,https://{bucket}.s3.amazonaws.com/{key}\n"
            ));
        }
        csv
    }

    fn parse_output_csv(csv_bytes: &[u8]) -> Vec<Vec<String>> {
        let mut rdr = csv::ReaderBuilder::new().from_reader(csv_bytes);
        rdr.records()
            .map(|r| r.unwrap().iter().map(|f| f.to_string()).collect())
            .collect()
    }

    #[tokio::test]
    async fn test_bucket_from_csv_key() {
        let file = File::new("managed", "reports/latest/manifests/my-bucket.csv");
        assert_eq!(bucket_from_csv_key(&file).unwrap(), "my-bucket");
    }

    #[tokio::test]
    async fn test_bucket_from_csv_key_invalid() {
        let file = File::new("managed", "reports/latest/manifests/no-extension");
        assert!(bucket_from_csv_key(&file).is_err());
    }

    #[tokio::test]
    async fn test_download_failure_aborts() {
        let sdk_config = TestClientBuilder::new()
            .s3_error("NoSuchKey", "csv not found")
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let result = perform(&config, &csv_file(&config), &PerformOptions::default()).await;
        assert!(
            matches!(result, Err(ChecksumInventoryError::Download(_))),
            "should abort on CSV download failure: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_error_status_on_non_404_failure() {
        let csv = inventory_csv(&[("test-bucket", "denied.jpg", "9999")]);

        let sdk_config = TestClientBuilder::new()
            .success(SdkBody::from(csv), None)
            .s3_error("AccessDenied", "forbidden")
            .ok()
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let result = perform(&config, &csv_file(&config), &PerformOptions::default()).await;
        assert!(
            result.is_ok(),
            "perform should not abort on non-404 HEAD error: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_missing_checksum_status() {
        let csv = inventory_csv(&[("test-bucket", "no-crc.jpg", "5678")]);

        let sdk_config = TestClientBuilder::new()
            .success(SdkBody::from(csv), None)
            .ok() // HEAD 200, no checksum header
            .ok()
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let result = perform(&config, &csv_file(&config), &PerformOptions::default()).await;
        assert!(
            result.is_ok(),
            "perform should not abort on missing checksum: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_not_found_status() {
        let csv = inventory_csv(&[("test-bucket", "deleted.jpg", "1234")]);

        let sdk_config = TestClientBuilder::new()
            .success(SdkBody::from(csv), None)
            .error(404, "NotFound", "not found")
            .ok()
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let result = perform(&config, &csv_file(&config), &PerformOptions::default()).await;
        assert!(
            result.is_ok(),
            "perform should not abort on 404: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_ok_status_with_checksum() {
        let csv = inventory_csv(&[("test-bucket", "file.jpg", "1234")]);

        let sdk_config = TestClientBuilder::new()
            .success(SdkBody::from(csv), None)
            .success_with_headers(SdkBody::empty(), &[CHECKSUM_HEADER])
            .ok()
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let result = perform(&config, &csv_file(&config), &PerformOptions::default()).await;
        assert!(result.is_ok(), "perform should succeed: {result:?}");
    }

    #[tokio::test]
    async fn test_output_csv_contains_correct_statuses() {
        let csv = inventory_csv(&[
            ("test-bucket", "good.jpg", "100"),
            ("test-bucket", "deleted.jpg", "200"),
            ("test-bucket", "no-crc.jpg", "300"),
        ]);

        let (sdk_config, replay) = TestClientBuilder::new()
            .success(SdkBody::from(csv), None)
            .success_with_headers(SdkBody::empty(), &[CHECKSUM_HEADER])
            .error(404, "NotFound", "not found")
            .ok() // HEAD 200, no checksum header
            .ok() // upload
            .build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);

        perform(&config, &csv_file(&config), &PerformOptions::default())
            .await
            .expect("perform should succeed");

        let requests = test_support::recorded_requests(&replay);
        let put = requests
            .iter()
            .find(|r| r.method == "PUT")
            .expect("should have a PUT request");

        let rows = parse_output_csv(&put.body);
        assert_eq!(rows.len(), 3, "should have 3 data rows");

        // buffer_unordered doesn't preserve order, so check by key
        for row in &rows {
            match row[1].as_str() {
                "good.jpg" => {
                    assert_eq!(row[2], TEST_CHECKSUM);
                    assert_eq!(row[4], "ok");
                }
                "deleted.jpg" => {
                    assert_eq!(row[2], "");
                    assert_eq!(row[4], "not_found");
                }
                "no-crc.jpg" => {
                    assert_eq!(row[2], "");
                    assert_eq!(row[4], "missing_checksum");
                }
                key => panic!("unexpected key: {key}"),
            }
        }
    }
}
