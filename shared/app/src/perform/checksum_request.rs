use awsutils::file::{self, File};
use base::stack::DateCtx;
use bytes::Bytes;
use constants::TEXT_CSV;

use crate::{bucket, checksum, config::Config, errors::ChecksumRequestError, upload};

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub csv_file: File,
    pub date_ctx: DateCtx,
}

impl PerformArgs {
    pub fn new(csv_file: File) -> Self {
        Self {
            csv_file,
            date_ctx: DateCtx::Today,
        }
    }
}

pub async fn perform(config: &Config, args: &PerformArgs) -> Result<String, ChecksumRequestError> {
    let csv_file = &args.csv_file;
    let bucket = bucket::name_from_file(csv_file)?;

    if !file::exists(config.s3(), csv_file)
        .await
        .map_err(ChecksumRequestError::Download)?
    {
        tracing::warn!("Inventory report not found: {}", csv_file.s3_url());
        return Err(ChecksumRequestError::InventoryNotFound(csv_file.s3_url()));
    }

    let bytes = file::download_bytes(config.s3(), csv_file)
        .await
        .map_err(ChecksumRequestError::Download)?;

    let rows = checksum::parse_inventory_rows(&bytes);
    let (csv_bytes, count, skipped) = checksum::generate_inventory(config, rows).await?;

    tracing::info!("Processed {count} inventory rows");

    if skipped > 0 {
        tracing::warn!("{skipped} of {count} objects had non-ok status");
    }

    let csv_bytes = Bytes::from(csv_bytes);
    let output_name = format!("{bucket}_{}", "checksum-inventory");

    upload::put_versioned_bytes(
        config,
        args.date_ctx,
        csv_bytes,
        TEXT_CSV,
        |ctx| config.stack().reports_checksums_path(&output_name, ctx),
        ChecksumRequestError::Upload,
    )
    .await?;

    let latest_file = File::from(
        config
            .stack()
            .reports_checksums_path(&output_name, DateCtx::Latest),
    );

    Ok(latest_file.s3_url())
}

#[cfg(test)]
mod tests {
    use std::{assert_matches, collections::HashMap};

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

    fn csv_args(config: &Config) -> PerformArgs {
        PerformArgs::new(csv_file(config))
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

    fn parse_output_csv(csv_bytes: &[u8]) -> Vec<HashMap<String, String>> {
        let mut rdr = csv::ReaderBuilder::new().from_reader(csv_bytes);
        let headers: Vec<String> = rdr
            .headers()
            .unwrap()
            .iter()
            .map(|h| h.to_string())
            .collect();
        rdr.records()
            .map(|r| {
                let record = r.unwrap();
                headers
                    .iter()
                    .zip(record.iter())
                    .map(|(h, v)| (h.clone(), v.to_string()))
                    .collect()
            })
            .collect()
    }

    #[tokio::test]
    async fn test_bucket_from_file() {
        let file = File::new("managed", "reports/latest/manifests/my-bucket.csv");
        assert_eq!(bucket::name_from_file(&file).unwrap(), "my-bucket");
    }

    #[tokio::test]
    async fn test_bucket_from_file_invalid() {
        let file = File::new("managed", "reports/latest/manifests/no-extension");
        assert!(bucket::name_from_file(&file).is_err());
    }

    #[tokio::test]
    async fn test_missing_inventory_report_aborts_before_download() {
        let sdk_config = TestClientBuilder::new()
            .s3_error("NoSuchKey", "csv not found")
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let args = csv_args(&config);
        let result = perform(&config, &args).await;
        assert_matches!(
            result,
            Err(ChecksumRequestError::InventoryNotFound(_)),
            "should abort before downloading a missing CSV: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_download_failure_after_exists_check_aborts() {
        let sdk_config = TestClientBuilder::new()
            .ok()
            .s3_error("NoSuchKey", "csv not found")
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let args = csv_args(&config);
        let result = perform(&config, &args).await;
        assert_matches!(
            result,
            Err(ChecksumRequestError::Download(_)),
            "should abort on CSV download failure after existence check: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_error_status_on_non_404_failure() {
        let csv = inventory_csv(&[("test-bucket", "denied.jpg", "9999")]);

        let (sdk_config, replay) = TestClientBuilder::new()
            .ok()
            .success(SdkBody::from(csv), None)
            .s3_error("AccessDenied", "forbidden")
            .ok()
            .ok()
            .build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);

        let args = csv_args(&config);
        let result = perform(&config, &args).await;
        assert!(
            result.is_ok(),
            "perform should not abort on non-404 HEAD error: {result:?}"
        );

        let requests = test_support::recorded_requests(&replay);
        let put = requests
            .iter()
            .find(|r| r.method == "PUT")
            .expect("should have a PUT request");
        let rows = parse_output_csv(&put.body);
        let row = rows
            .iter()
            .find(|r| r["key"] == "denied.jpg")
            .expect("row for denied.jpg");
        assert_eq!(row["status"], "error");
        assert!(
            row["detail"].contains("AccessDenied"),
            "detail should surface the underlying error code, got: {:?}",
            row["detail"]
        );
    }

    #[tokio::test]
    async fn test_missing_checksum_status() {
        let csv = inventory_csv(&[("test-bucket", "no-crc.jpg", "5678")]);

        let sdk_config = TestClientBuilder::new()
            .ok()
            .success(SdkBody::from(csv), None)
            .ok() // HEAD 200, no checksum header
            .ok()
            .ok()
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let args = csv_args(&config);
        let result = perform(&config, &args).await;
        assert!(
            result.is_ok(),
            "perform should not abort on missing checksum: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_not_found_status() {
        let csv = inventory_csv(&[("test-bucket", "deleted.jpg", "1234")]);

        let sdk_config = TestClientBuilder::new()
            .ok()
            .success(SdkBody::from(csv), None)
            .error(404, "NotFound", "not found")
            .ok()
            .ok()
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let args = csv_args(&config);
        let result = perform(&config, &args).await;
        assert!(
            result.is_ok(),
            "perform should not abort on 404: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_ok_status_with_checksum() {
        let csv = inventory_csv(&[("test-bucket", "file.jpg", "1234")]);

        let sdk_config = TestClientBuilder::new()
            .ok()
            .success(SdkBody::from(csv), None)
            .success_with_headers(SdkBody::empty(), &[CHECKSUM_HEADER])
            .ok()
            .ok()
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let args = csv_args(&config);
        let result = perform(&config, &args).await;
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
            .ok()
            .success(SdkBody::from(csv), None)
            .success_with_headers(SdkBody::empty(), &[CHECKSUM_HEADER])
            .error(404, "NotFound", "not found")
            .ok() // HEAD 200, no checksum header
            .ok() // upload latest
            .ok() // upload dated
            .build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);

        let args = csv_args(&config);
        perform(&config, &args)
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
            match row["key"].as_str() {
                "good.jpg" => {
                    assert_eq!(row["checksum"], TEST_CHECKSUM);
                    assert_eq!(row["checksum_algorithm"], "crc64nvme");
                    assert_eq!(row["status"], "ok");
                }
                "deleted.jpg" => {
                    assert_eq!(row["checksum"], "");
                    assert_eq!(row["checksum_algorithm"], "");
                    assert_eq!(row["status"], "not_found");
                }
                "no-crc.jpg" => {
                    assert_eq!(row["checksum"], "");
                    assert_eq!(row["checksum_algorithm"], "");
                    assert_eq!(row["status"], "missing_checksum");
                }
                key => panic!("unexpected key: {key}"),
            }
        }
    }
}
