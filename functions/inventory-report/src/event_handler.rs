use app::{config::Config, perform::inventory_report};
use aws_lambda_events::event::s3::S3Event;
use awsutils::file::File;
use lambda_runtime::{Error, LambdaEvent, tracing};

pub(crate) async fn function_handler(
    config: &Config,
    event: LambdaEvent<S3Event>,
) -> Result<(), Error> {
    let payload = event.payload;

    let record = payload.records.first().expect("payload should have record");
    let bucket = record.s3.bucket.name.as_ref().expect("bucket required");
    let object = record.s3.object.key.as_ref().expect("object requried");

    tracing::info!("Bucket: {:?}, Object: {:?}", bucket, object);

    if bucket != &config.stack().managed_bucket() {
        panic!(
            "Not the managed bucket for this stack: {:?}",
            config.stack()
        );
    }

    if !object.ends_with("manifest.json") {
        panic!("Not an inventory manifest file: {:?}", object);
    }

    if config.debug_handler() {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    let args = inventory_report::PerformArgs::new(File::new(bucket, object));
    let stats = inventory_report::perform(config, &args).await?;

    tracing::info!(
        "Processed {} files, {} bytes total",
        stats.total_files,
        stats.total_size
    );

    if stats.replication_errors > 0 {
        return Err(std::io::Error::other(format!(
            "Inventory manifest s3://{bucket}/{object} contains {} replication failures",
            stats.replication_errors
        ))
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::config as app_config;
    use lambda_runtime::{Context, LambdaEvent};
    use test_support::TestClientBuilder;

    #[tokio::test]
    #[should_panic(expected = "Not an inventory manifest file")]
    async fn test_invalid_event_handler() {
        let json = include_str!("../events/sample.json");
        let mut s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        // make it so object key != the expected manifest.json
        s3_event.records[0].s3.object.key = Some("something-else.json".to_string());

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        function_handler(&config, event).await.unwrap();
    }

    #[tokio::test]
    async fn test_valid_event_handler() {
        // json contains object key == manifest.json
        let json = include_str!("../events/sample.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        function_handler(&config, event).await.unwrap();
    }

    #[tokio::test]
    async fn test_replication_failures_return_error_after_reports_are_saved() {
        for (status, expected_errors) in [("FAILED", 1), ("COMPLETED", 0)] {
            let temp_dir = tempfile::tempdir().unwrap();
            let parquet_path = temp_dir.path().join("inventory.parquet");
            let conn = duckdb::Connection::open_in_memory().unwrap();
            conn.execute_batch(&format!(
                r#"
                COPY (SELECT
                    'test-stack-private' AS bucket,
                    'file.txt' AS key,
                    100::BIGINT AS size,
                    TIMESTAMPTZ '2025-01-01 00:00:00+00' AS last_modified_date,
                    'STANDARD' AS storage_class,
                    '{status}' AS replication_status
                ) TO {} (FORMAT PARQUET)
                "#,
                base::safe_join(&[parquet_path.to_string_lossy()])
            ))
            .unwrap();
            let parquet_bytes = std::fs::read(&parquet_path).unwrap();
            let manifest = serde_json::json!({
                "sourceBucket": "test-stack-private",
                "destinationBucket": "arn:aws:s3:::test-stack-managed",
                "version": "2016-11-30",
                "creationTimestamp": "1766538000000",
                "fileFormat": "Parquet",
                "fileSchema": "message s3.inventory {}",
                "files": [{
                    "key": "inventory/data/test.parquet",
                    "size": parquet_bytes.len(),
                    "MD5checksum": "ccfad504bdd9a835cf04e781b7a7ed16"
                }]
            });
            let copy_result = r#"<CopyObjectResult><ETag>"etag"</ETag></CopyObjectResult>"#;
            let (sdk_config, replay) = TestClientBuilder::new()
                .success(manifest.to_string(), None)
                .success(parquet_bytes, None)
                .ok()
                .success(copy_result, None)
                .ok()
                .success(copy_result, None)
                .build_sdk_config_with_replay();
            let config = app_config::Config::for_tests(sdk_config, false);
            let mut s3_event: S3Event =
                serde_json::from_str(include_str!("../events/sample.json")).unwrap();
            s3_event.records[0].s3.bucket.name = Some(config.stack().managed_bucket());
            let event = LambdaEvent::new(s3_event, Context::default());

            let result = function_handler(&config, event).await;
            if expected_errors > 0 {
                let error = result.expect_err("replication failures should fail the invocation");
                assert!(error.to_string().contains("1 replication failures"));
            } else {
                result.unwrap();
            }

            let requests = test_support::recorded_requests(&replay);
            assert_eq!(requests.iter().filter(|r| r.method == "PUT").count(), 4);
            let stats_request = requests
                .iter()
                .find(|r| r.content_type.as_deref() == Some("application/json"))
                .unwrap();
            let stats: base::stats::InventoryStats =
                serde_json::from_slice(&stats_request.body).unwrap();
            assert_eq!(stats.replication_errors, expected_errors);
        }
    }
}
