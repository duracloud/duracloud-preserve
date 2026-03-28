use aws_sdk_s3::Client;
use serde::Deserialize;

use crate::{
    errors::InventoryError,
    file::{self, File},
};

// Re-export processing types from base
pub use base::inventory::process;
pub use base::stats::{InventoryStats, PrefixStats};

/// Inventory Manifest
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryManifest {
    pub source_bucket: String,
    pub destination_bucket: String,
    pub version: String,
    pub creation_timestamp: String,
    pub file_format: String,
    pub file_schema: String,
    pub files: Vec<InventoryFileEntry>,
}

impl InventoryManifest {
    pub async fn fetch(client: &Client, file: &File) -> Result<Self, InventoryError> {
        let bytes = file::download_bytes(client, file).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

/// Inventory Manifest File Entry
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFileEntry {
    pub key: String,
    pub size: u64,
    #[serde(rename = "MD5checksum")]
    pub md5_checksum: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_manifest() {
        let json = include_str!("../../../files/inventory-manifest.json");
        let manifest: InventoryManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.source_bucket, "test-stack-private");
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].key, "example.parquet");
    }
}
