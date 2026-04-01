use std::collections::BTreeMap;

use awsutils::file::{self, File};
use base::{
    bucket::Type,
    stack::DateCtx,
    stats::InventoryStats,
    storage::{StorageReport, StorageReportData, StorageReportHeader},
};
use bytes::Bytes;
use chrono::Utc;
use constants::{APPLICATION_JSON, TEXT_HTML};

use crate::{bucket, config::Config, errors::StorageReportError, upload};

#[derive(Debug, Clone, Copy, Default)]
pub struct PerformOptions {
    pub storage_capacity_bytes: Option<u64>,
}

pub async fn perform(
    config: &Config,
    opts: &PerformOptions,
) -> Result<StorageReport, StorageReportError> {
    let buckets = bucket::list_for_stack_by_type(
        config.s3(),
        config.stack(),
        &[Type::Public, Type::Standard],
    )
    .await
    .map_err(StorageReportError::BucketDiscovery)?;
    let mut bucket_stats = BTreeMap::new();

    for bucket in buckets {
        let bucket_name = bucket.name().to_string();
        let bucket_type = bucket.bucket_type();

        tracing::info!("Retrieving inventory stats for: {bucket_name} {bucket_type}");

        let stats_file = File::from(
            config
                .stack()
                .metadata_manifests_stats_path(&bucket_name, DateCtx::Latest),
        );

        let stats = file::download_bytes(config.s3(), &stats_file)
            .await
            .map_err(|source| StorageReportError::DownloadStats {
                bucket: bucket_name.clone(),
                source,
            })?;

        let stats: InventoryStats =
            serde_json::from_slice(&stats).map_err(|source| StorageReportError::ParseStats {
                bucket: bucket_name.clone(),
                source,
            })?;

        bucket_stats.insert(bucket_name, stats);
    }

    let header = StorageReportHeader {
        owner: config.owner().to_string(),
        stack_name: config.stack().as_str().to_string(),
        generated_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        storage_capacity_bytes: opts.storage_capacity_bytes,
    };
    let data = StorageReportData::from_inventory(bucket_stats);
    let storage_report = StorageReport { header, data };

    let stats_bytes = Bytes::from(serde_json::to_vec(&storage_report)?);
    let html_bytes = Bytes::from(storage_report.to_html()?);

    upload::put_versioned_bytes(
        config,
        DateCtx::Today,
        html_bytes,
        TEXT_HTML,
        |ctx| config.stack().reports_storage_path(ctx),
        StorageReportError::UploadError,
    )
    .await?;

    upload::put_versioned_bytes(
        config,
        DateCtx::Today,
        stats_bytes,
        APPLICATION_JSON,
        |ctx| config.stack().metadata_storage_stats_path(ctx),
        StorageReportError::UploadError,
    )
    .await?;

    Ok(storage_report)
}
