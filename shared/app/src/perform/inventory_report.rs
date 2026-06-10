use base::stack::DateCtx;
use base::stats::InventoryStats;
use bytes::Bytes;
use constants::{APPLICATION_JSON, TEXT_CSV};

use awsutils::{
    bucket_creator::INVENTORY_FORMAT,
    file::File,
    inventory::{self, InventoryManifest},
};

use crate::{config::Config, errors::InventoryReportError, inventory as app_inventory, upload};
#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub manifest_file: File,
    pub date_ctx: DateCtx,
}

impl PerformArgs {
    pub fn new(manifest_file: File) -> Self {
        Self {
            manifest_file,
            date_ctx: DateCtx::Today,
        }
    }
}

pub async fn perform(
    config: &Config,
    args: &PerformArgs,
) -> Result<InventoryStats, InventoryReportError> {
    let manifest_file = &args.manifest_file;
    tracing::info!("Retrieving manifest file: {}", manifest_file.s3_url());
    let manifest = InventoryManifest::fetch(config.s3(), manifest_file).await?;
    manifest.require_format(INVENTORY_FORMAT.as_str())?;

    let temp_dir = tempfile::tempdir()?;
    let paths = app_inventory::download_parquets(config, &manifest, &temp_dir).await?;

    tracing::info!("Processing parquet files: {:?}", paths);
    let output_csv = temp_dir.path().join("inventory-report.csv");
    let stats = {
        let output_csv = output_csv.clone();
        tokio::task::spawn_blocking(move || inventory::process(&paths, &output_csv))
            .await
            .expect("spawn_blocking task panicked")?
    };

    let stats_bytes = Bytes::from(serde_json::to_vec(&stats)?);

    upload::put_versioned_file(
        config,
        args.date_ctx,
        &output_csv,
        TEXT_CSV,
        |ctx| {
            config
                .stack()
                .reports_manifests_path(&manifest.source_bucket, ctx)
        },
        InventoryReportError::Upload,
    )
    .await?;

    upload::put_versioned_bytes(
        config,
        args.date_ctx,
        stats_bytes,
        APPLICATION_JSON,
        |ctx| {
            config
                .stack()
                .metadata_manifests_stats_path(&manifest.source_bucket, ctx)
        },
        InventoryReportError::Upload,
    )
    .await?;

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use aws_smithy_types::body::SdkBody;

    use super::*;
    use crate::config as app_config;
    use test_support::TestClientBuilder;

    fn manifest_json(file_format: &str, parquet_key: &str, parquet_size: usize) -> String {
        serde_json::json!({
            "sourceBucket": "test-stack-private",
            "destinationBucket": "arn:aws:s3:::test-stack-managed",
            "version": "2016-11-30",
            "creationTimestamp": "1766538000000",
            "fileFormat": file_format,
            "fileSchema": "message s3.inventory {}",
            "files": [
                {
                    "key": parquet_key,
                    "size": parquet_size,
                    "MD5checksum": "ccfad504bdd9a835cf04e781b7a7ed16"
                }
            ]
        })
        .to_string()
    }

    fn uri_has_key(uri: &str, key: &str) -> bool {
        let encoded_upper = key.replace('/', "%2F");
        let encoded_lower = key.replace('/', "%2f");
        uri.contains(key) || uri.contains(&encoded_upper) || uri.contains(&encoded_lower)
    }

    const COPY_RESULT_XML: &str = r#"<CopyObjectResult><ETag>"etag"</ETag></CopyObjectResult>"#;

    #[tokio::test]
    async fn test_perform_processes_manifest_and_writes_latest_and_dated_outputs() {
        let manifest_key = "inventory-report-tests/input-manifest.json";
        let parquet_key = "inventory-report-tests/example.parquet";
        let source_bucket = "test-stack-private";

        let parquet_bytes = include_bytes!("../../../../files/example.parquet");
        let manifest = manifest_json(INVENTORY_FORMAT.as_str(), parquet_key, parquet_bytes.len());

        // StaticReplayClient responds in sequence: manifest GET, parquet GET,
        // dated csv PUT, csv copy to LATEST, dated stats PUT, stats copy to LATEST.
        let (sdk_config, replay) = TestClientBuilder::new()
            .success(manifest, None)
            .success(SdkBody::from(parquet_bytes.to_vec()), None)
            .ok()
            .success(COPY_RESULT_XML, None)
            .ok()
            .success(COPY_RESULT_XML, None)
            .build_sdk_config_with_replay();

        let config = app_config::Config::for_tests(sdk_config, false);
        let manifest_file = File::new(config.stack().managed_bucket(), manifest_key);
        let args = PerformArgs::new(manifest_file);
        let stats = perform(&config, &args)
            .await
            .expect("perform should succeed");

        assert_eq!(stats.total_files, 13);
        assert_eq!(stats.total_size, 2_191_162);

        let requests = test_support::recorded_requests(&replay);

        assert!(
            requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, manifest_key))
        );
        assert!(
            requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, parquet_key))
        );

        // CSV: one streamed PUT to the dated key, one server-side copy to LATEST.
        let dated_csv_key = config
            .stack()
            .reports_manifests_path(source_bucket, DateCtx::Today);
        let latest_csv_key = config
            .stack()
            .reports_manifests_path(source_bucket, DateCtx::Latest);

        let csv_puts: Vec<_> = requests
            .iter()
            .filter(|r| r.method == "PUT" && r.content_type.as_deref() == Some(TEXT_CSV))
            .collect();
        assert_eq!(csv_puts.len(), 1);
        assert!(uri_has_key(&csv_puts[0].uri, dated_csv_key.key()));
        assert!(csv_puts[0].copy_source.is_none());

        let csv_copies: Vec<_> = requests
            .iter()
            .filter(|r| {
                r.copy_source
                    .as_deref()
                    .is_some_and(|src| src.ends_with(dated_csv_key.key()))
            })
            .collect();
        assert_eq!(csv_copies.len(), 1);
        assert_eq!(csv_copies[0].method, "PUT");
        assert!(uri_has_key(&csv_copies[0].uri, latest_csv_key.key()));

        // Stats JSON: same PUT + copy pattern; the in-memory body is recorded.
        let dated_stats_key = config
            .stack()
            .metadata_manifests_stats_path(source_bucket, DateCtx::Today);
        let latest_stats_key = config
            .stack()
            .metadata_manifests_stats_path(source_bucket, DateCtx::Latest);

        let stats_puts: Vec<_> = requests
            .iter()
            .filter(|r| r.method == "PUT" && r.content_type.as_deref() == Some(APPLICATION_JSON))
            .collect();
        assert_eq!(stats_puts.len(), 1);
        assert!(uri_has_key(&stats_puts[0].uri, dated_stats_key.key()));
        assert!(stats_puts[0].copy_source.is_none());

        let stats_copies: Vec<_> = requests
            .iter()
            .filter(|r| {
                r.copy_source
                    .as_deref()
                    .is_some_and(|src| src.ends_with(dated_stats_key.key()))
            })
            .collect();
        assert_eq!(stats_copies.len(), 1);
        assert!(uri_has_key(&stats_copies[0].uri, latest_stats_key.key()));

        let uploaded_stats_json: serde_json::Value = serde_json::from_slice(&stats_puts[0].body)
            .expect("uploaded stats should be valid json");
        let expected_stats_json =
            serde_json::to_value(&stats).expect("stats should serialize to json");
        assert_eq!(uploaded_stats_json, expected_stats_json);
    }

    #[tokio::test]
    async fn test_perform_rejects_non_parquet_manifest_format() {
        let manifest_key = "inventory-report-tests/invalid-format-manifest.json";
        let parquet_key = "inventory-report-tests/example.csv";
        let manifest = manifest_json("CSV", parquet_key, 10);

        let (sdk_config, replay) = TestClientBuilder::new()
            .success(manifest, None)
            .build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);
        let manifest_file = File::new(config.stack().managed_bucket(), manifest_key);
        let args = PerformArgs::new(manifest_file);

        let err = perform(&config, &args)
            .await
            .expect_err("perform should fail for non-parquet format");
        match err {
            InventoryReportError::InvalidFormat { expected, actual } => {
                assert_eq!(expected, INVENTORY_FORMAT.to_string());
                assert_eq!(actual, "CSV");
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let requests = test_support::recorded_requests(&replay);

        assert!(
            requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, manifest_key))
        );
        assert!(
            !requests
                .iter()
                .any(|r| r.method == "GET" && uri_has_key(&r.uri, parquet_key))
        );
        assert!(!requests.iter().any(|r| r.method == "PUT"));
    }
}
