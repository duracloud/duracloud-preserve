use awsutils::file::{self, File};
use base::stack::DateCtx;

use crate::config::Config;

#[cfg(feature = "duckdb")]
use awsutils::inventory::InventoryManifest;
#[cfg(feature = "duckdb")]
use tempfile::TempDir;

#[cfg(feature = "duckdb")]
use crate::errors::InventoryReportError;

/// Download every parquet referenced by `manifest` into `temp_dir` and return
/// the local paths as owned strings ready for a `spawn_blocking` call.
#[cfg(feature = "duckdb")]
pub async fn download_parquets(
    config: &Config,
    manifest: &InventoryManifest,
    temp_dir: &TempDir,
) -> Result<Vec<String>, InventoryReportError> {
    let bucket = config.stack().managed_bucket();
    let files = manifest
        .files
        .iter()
        .map(|entry| File::new(&bucket, &entry.key))
        .collect::<Vec<_>>();

    let local_paths =
        file::download_files_to_temp(config.s3(), &files, temp_dir, "inventory manifest file")
            .await
            .map_err(InventoryReportError::Download)?;

    Ok(local_paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// Determine which inventory to use: today's if available, otherwise yesterday's.
pub async fn get_manifest(config: &Config, target_bucket: &str) -> Result<File, &'static str> {
    for ctx in [DateCtx::Today, DateCtx::Yesterday] {
        let manifest = File::from(config.stack().inventory_manifest_path(target_bucket, ctx));
        println!("Checking for manifest: {}", manifest.s3_url());
        if file::exists(config.s3(), &manifest).await {
            return Ok(manifest);
        }
    }
    Err("No inventory manifest found for today or yesterday")
}
