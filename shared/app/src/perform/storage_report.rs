use base::{bucket::Type, stack::DateCtx, storage::StorageReport};
use bytes::Bytes;
use constants::{APPLICATION_JSON, TEXT_HTML};

use crate::{bucket, config::Config, errors::StorageReportError, upload};

#[derive(Debug, Clone, Copy, Default)]
pub struct PerformArgs {
    pub storage_capacity_bytes: Option<u64>,
}

pub async fn perform(
    config: &Config,
    args: &PerformArgs,
) -> Result<StorageReport, StorageReportError> {
    let buckets = bucket::list_for_stack_by_type(
        config.s3(),
        config.stack(),
        &[Type::Public, Type::Standard],
    )
    .await
    .map_err(StorageReportError::BucketDiscovery)?;

    let bucket_stats = bucket::fetch_latest_inventory_stats(config, buckets).await?;

    let storage_report = StorageReport::assemble(
        config.owner().to_string(),
        config.stack().as_str().to_string(),
        args.storage_capacity_bytes,
        bucket_stats,
    );

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
