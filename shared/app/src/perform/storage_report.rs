use std::collections::BTreeMap;

use apputils::{
    bucket::Type,
    content_type::{APPLICATION_JSON, TEXT_HTML},
    stack,
    stats::InventoryStats,
    storage::{StorageReport, StorageReportMeta},
};
use aws_sdk_s3::primitives::ByteStream;
use awsutils::file::{self, File, download_bytes};
use bytes::Bytes;
use chrono::Utc;

use crate::{bucket::get_stack_buckets_by_type, config::Config, errors::StorageReportError};

#[derive(Debug, Clone, Copy, Default)]
pub struct PerformOptions {
    pub storage_capacity_bytes: Option<u64>,
}

pub async fn perform(
    config: &Config,
    opts: &PerformOptions,
) -> Result<StorageReport, StorageReportError> {
    let buckets =
        get_stack_buckets_by_type(config.s3(), config.stack(), &[Type::Public, Type::Standard])
            .await
            .map_err(StorageReportError::BucketDiscovery)?;
    let mut bucket_stats = BTreeMap::new();

    for bucket in buckets {
        let bucket_name = bucket.name().to_string();
        let bucket_type = bucket.bucket_type();

        tracing::info!("Retrieving inventory stats for: {bucket_name} {bucket_type}");

        let stats_path = config
            .stack()
            .metadata_manifests_stats_path(&bucket_name, stack::DateCtx::Latest);

        let stats = download_bytes(
            config.s3(),
            &File::new(config.stack().managed_bucket(), stats_path),
        )
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

    let storage_report = StorageReport::from_inventory(bucket_stats);
    let managed_bucket = config.stack().managed_bucket();
    let meta = StorageReportMeta {
        stack_name: config.stack().as_str().to_string(),
        generated_at: Utc::now().format("%m/%d/%Y %H:%M:%S UTC").to_string(),
        storage_capacity_bytes: opts.storage_capacity_bytes,
    };

    let stats_bytes = Bytes::from(serde_json::to_vec(&storage_report)?);
    let html_bytes = Bytes::from(storage_report.to_html(meta)?);

    for ctx in [stack::DateCtx::Latest, stack::DateCtx::Today] {
        let html_path = config.stack().reports_storage_path(ctx);
        let html_file = File::new(&managed_bucket, html_path);

        tracing::info!("Uploading html: {}", html_file.s3_url());
        file::upload(
            config.s3(),
            &html_file,
            ByteStream::from(html_bytes.clone()),
            TEXT_HTML,
        )
        .await
        .map_err(StorageReportError::UploadError)?;

        let stats_path = config.stack().metadata_storage_stats_path(ctx);
        let stats_file = File::new(&managed_bucket, stats_path);

        tracing::info!("Uploading stats: {}", stats_file.s3_url());
        file::upload(
            config.s3(),
            &stats_file,
            ByteStream::from(stats_bytes.clone()),
            APPLICATION_JSON,
        )
        .await
        .map_err(StorageReportError::UploadError)?;
    }

    Ok(storage_report)
}
