use apputils::stack::DateCtx;
use apputils::stats::InventoryStats;
use bytes::Bytes;
use constants::{APPLICATION_JSON, TEXT_CSV};

use awsutils::{
    bucket_creator::INVENTORY_FORMAT,
    file::{self, File},
    inventory::{self, InventoryManifest},
};

use crate::{config::Config, errors::InventoryReportError, upload};
#[derive(Debug, Clone, Copy)]
pub struct PerformOptions {
    pub date_ctx: DateCtx,
}

impl Default for PerformOptions {
    fn default() -> Self {
        Self {
            date_ctx: DateCtx::Today,
        }
    }
}

pub async fn perform(
    config: &Config,
    manifest_file: &File,
    opts: &PerformOptions,
) -> Result<InventoryStats, InventoryReportError> {
    tracing::info!("Retrieving manifest file: {}", manifest_file.s3_url());
    let manifest = InventoryManifest::fetch(config.s3(), manifest_file).await?;
    let bucket = config.stack().managed_bucket();

    if manifest.file_format != INVENTORY_FORMAT.as_str() {
        return Err(InventoryReportError::InvalidFormat {
            expected: INVENTORY_FORMAT.to_string(),
            actual: manifest.file_format.clone(),
        });
    }

    let temp_dir = tempfile::tempdir()?;
    let files = manifest
        .files
        .iter()
        .map(|entry| File::new(&bucket, &entry.key))
        .collect::<Vec<_>>();

    let local_paths =
        file::download_files_to_temp(config.s3(), &files, &temp_dir, "inventory manifest file")
            .await
            .map_err(InventoryReportError::Download)?;

    let path_strs_owned: Vec<String> = local_paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    tracing::info!("Processing parquet files: {:?}", path_strs_owned);
    let (csv, stats) = tokio::task::spawn_blocking(move || inventory::process(&path_strs_owned))
        .await
        .expect("spawn_blocking task panicked")?;

    let csv_bytes = Bytes::from(csv);
    let stats_bytes = Bytes::from(serde_json::to_vec(&stats)?);

    upload::put_versioned_bytes(
        config,
        opts.date_ctx,
        csv_bytes,
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
        opts.date_ctx,
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
    use std::collections::HashSet;

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

    #[tokio::test]
    async fn test_perform_processes_manifest_and_writes_latest_and_dated_outputs() {
        let manifest_key = "inventory-report-tests/input-manifest.json";
        let parquet_key = "inventory-report-tests/example.parquet";
        let source_bucket = "test-stack-private";

        let parquet_bytes = include_bytes!("../../../../files/example.parquet");
        let manifest = manifest_json(INVENTORY_FORMAT.as_str(), parquet_key, parquet_bytes.len());

        // StaticReplayClient responds in sequence, so key naming here is for readability/safety.
        let (sdk_config, replay) = TestClientBuilder::new()
            .success(manifest, None)
            .success(SdkBody::from(parquet_bytes.to_vec()), None)
            .ok()
            .ok()
            .ok()
            .ok()
            .build_sdk_config_with_replay();

        let config = app_config::Config::for_tests(sdk_config, false);
        let manifest_file = File::new(config.stack().managed_bucket(), manifest_key);
        let opts = PerformOptions::default();
        let stats = perform(&config, &manifest_file, &opts)
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

        let csv_puts: Vec<_> = requests
            .iter()
            .filter(|r| r.method == "PUT" && r.content_type.as_deref() == Some(TEXT_CSV))
            .collect();
        assert_eq!(csv_puts.len(), 2);

        let latest_csv_key = config
            .stack()
            .reports_manifests_path(source_bucket, DateCtx::Latest);
        assert!(
            csv_puts
                .iter()
                .any(|r| uri_has_key(&r.uri, latest_csv_key.key()))
        );
        assert!(
            csv_puts
                .iter()
                .any(|r| !uri_has_key(&r.uri, latest_csv_key.key()))
        );

        let csv_uris: HashSet<_> = csv_puts.iter().map(|r| r.uri.as_str()).collect();
        assert_eq!(csv_uris.len(), 2);
        assert!(csv_puts.iter().all(|r| r.body == csv_puts[0].body));

        let csv = std::str::from_utf8(&csv_puts[0].body).unwrap();
        assert!(csv.starts_with(
            "bucket,key,size,last_modified_date,storage_class,replication_status,url\n"
        ));
        assert!(csv.contains("documents/my report.pdf"));

        let stats_puts: Vec<_> = requests
            .iter()
            .filter(|r| r.method == "PUT" && r.content_type.as_deref() == Some(APPLICATION_JSON))
            .collect();
        assert_eq!(stats_puts.len(), 2);

        let latest_stats_key = config
            .stack()
            .metadata_manifests_stats_path(source_bucket, DateCtx::Latest);
        assert!(
            stats_puts
                .iter()
                .any(|r| uri_has_key(&r.uri, latest_stats_key.key()))
        );
        assert!(
            stats_puts
                .iter()
                .any(|r| !uri_has_key(&r.uri, latest_stats_key.key()))
        );

        let stats_uris: HashSet<_> = stats_puts.iter().map(|r| r.uri.as_str()).collect();
        assert_eq!(stats_uris.len(), 2);
        assert!(stats_puts.iter().all(|r| r.body == stats_puts[0].body));

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
        let opts = PerformOptions::default();

        let err = perform(&config, &manifest_file, &opts)
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
