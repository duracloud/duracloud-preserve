use apputils::stack::{self, DateCtx};
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;

use crate::{
    bucket_creator::INVENTORY_FORMAT,
    config::RequestConfig,
    file::{self, File},
    inventory::{InventoryError, InventoryManifest, InventoryStats, process},
};

pub async fn perform(
    config: &RequestConfig,
    manifest_file: &File,
    date_ctx: DateCtx,
) -> Result<InventoryStats, InventoryError> {
    tracing::info!("Retrieving manifest file: {:?}", manifest_file);
    let manifest = InventoryManifest::fetch(&config.client, manifest_file).await?;
    let bucket = config.stack().managed_bucket();

    if manifest.file_format != INVENTORY_FORMAT.as_str() {
        return Err(InventoryError::InvalidFormat {
            expected: INVENTORY_FORMAT.to_string(),
            actual: manifest.file_format.clone(),
        });
    }

    let temp_dir = tempfile::tempdir()?;
    let mut local_paths = Vec::new();

    for entry in &manifest.files {
        let file = File::new(&bucket, &entry.key);
        tracing::info!("Downloading inventory file: {:?}", file);

        let response = file::download(&config.client, &file)
            .await
            .map_err(|e| InventoryError::S3(Box::new(e)))?;
        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| InventoryError::S3(Box::new(e)))?
            .into_bytes();

        let filename = entry.key.rsplit('/').next().unwrap_or(&entry.key);
        let local_path = temp_dir.path().join(filename);
        std::fs::write(&local_path, &bytes)?;
        local_paths.push(local_path);
    }

    let path_strs: Vec<&str> = local_paths
        .iter()
        .map(|p| p.to_str().expect("valid utf-8 path"))
        .collect();

    tracing::info!("Processing parquet files: {:?}", path_strs);
    let (csv, stats) = process(&path_strs)?;

    let csv_bytes = Bytes::from(csv);
    let stats_bytes = Bytes::from(serde_json::to_vec(&stats)?);

    for ctx in [stack::DateCtx::Latest, date_ctx] {
        let csv_path = config
            .stack()
            .reports_manifest_path(&manifest.source_bucket, ctx);
        let csv_file = File::new(&bucket, csv_path);

        tracing::info!("Uploading csv: {:?}", csv_file);
        file::upload(
            &config.client,
            &csv_file,
            ByteStream::from(csv_bytes.clone()),
            "text/csv",
        )
        .await?;

        let stats_path = config
            .stack()
            .reports_stats_path(&manifest.source_bucket, ctx);
        let stats_file = File::new(&bucket, stats_path);

        tracing::info!("Uploading stats: {:?}", stats_file);
        file::upload(
            &config.client,
            &stats_file,
            ByteStream::from(stats_bytes.clone()),
            "application/json",
        )
        .await?;
    }

    Ok(stats)
}
