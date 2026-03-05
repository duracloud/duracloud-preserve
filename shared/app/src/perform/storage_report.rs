use std::collections::BTreeMap;

use apputils::{bucket::Type, stack, stats::InventoryStats, storage::StorageReport};
use awsutils::file::{File, download_bytes};

use crate::{
    bucket::get_stack_buckets_by_type, config::Config, perform::errors::StorageReportError,
};

pub async fn perform(config: &Config) -> Result<(), StorageReportError> {
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
    dbg!(storage_report);

    Ok(())
}
