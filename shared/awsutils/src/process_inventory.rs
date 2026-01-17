use apputils::stack;
use aws_sdk_s3::primitives::ByteStream;

use crate::{
    config::RequestConfig,
    file::{self, File},
    inventory::{InventoryError, InventoryManifest, InventoryStats, process},
};

pub async fn perform(
    config: &RequestConfig,
    manifest_file: &File,
) -> Result<InventoryStats, InventoryError> {
    tracing::info!("Retrieving manifest file: {:?}", manifest_file);
    let manifest = InventoryManifest::fetch(&config.s3_client, manifest_file).await?;
    let bucket = config.stack.managed_bucket();

    let temp_dir = tempfile::tempdir()?;
    let mut local_paths = Vec::new();

    for entry in &manifest.files {
        let file = File::new(bucket.clone(), entry.key.clone());
        tracing::info!("Downloading inventory file: {:?}", file);

        let response = file::download(&config.s3_client, &file)
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

    let mut csv_buffer = Vec::new();

    tracing::info!("Processing parquet files: {:?}", path_strs);
    let stats = process(&mut csv_buffer, &path_strs)?;

    let csv_path = config
        .stack
        .reports_manifest_path(&manifest.source_bucket, stack::DateCtx::Yesterday);
    let csv_file = File::new(bucket, csv_path);

    tracing::info!("Uploading csv: {:?}", csv_file);
    file::upload(
        &config.s3_client,
        &csv_file,
        ByteStream::from(csv_buffer),
        "text/csv",
    )
    .await?;

    Ok(stats)
}
