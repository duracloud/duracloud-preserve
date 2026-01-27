use aws_sdk_s3::Client;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    bucket::RequestError,
    file::{self, File},
};

// Re-export DuckDB processing types from apputils
pub use apputils::inventory::{
    InventoryError as ProcessingError, InventoryProcessor, InventoryStats, PrefixStats, process,
};

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("Invalid inventory format: expected '{expected}', got '{actual}'")]
    InvalidFormat { expected: String, actual: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Processing error: {0}")]
    Processing(#[from] ProcessingError),
    #[error("{0}")]
    Request(#[from] RequestError),
    #[error("S3 error: {0:#}")]
    S3(#[source] Box<dyn std::error::Error + Send + Sync>),
}

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
        let response = file::download(client, file)
            .await
            .map_err(|e| InventoryError::S3(Box::new(e)))?;
        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| InventoryError::S3(Box::new(e)))?
            .into_bytes();
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
