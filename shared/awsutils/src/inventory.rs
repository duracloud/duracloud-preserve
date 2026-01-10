use std::io::Write;

use aws_sdk_s3::Client;
use percent_encoding::percent_decode_str;
use polars::{
    error::PolarsError,
    frame::DataFrame,
    prelude::{
        ChunkApply, CsvWriter, IntoLazy, LazyFrame, PlPath, SerWriter, StringChunked, col, lit,
    },
    series::IntoSeries,
};
use polars_arrow::buffer::Buffer;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    config::RequestConfig,
    file::{File, download},
};

const INVENTORY_OBJECT_KEY: &str = "key";
const INVENTORY_SIZE_KEY: &str = "size";

pub async fn perform(_config: &RequestConfig, _manifest: &File) -> Result<(), InventoryError> {
    tracing::info!("Retrieving manifest file from S3");

    todo!()
}

pub fn process(
    mut csv_file: impl Write,
    parquet_files: &[&str],
) -> Result<InventoryStats, InventoryError> {
    InventoryProcessor::load(&parquet_files)?
        .decode_keys()?
        .write_csv(&mut csv_file)?
        .stats()
}

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Polars error: {0}")]
    Polars(#[from] PolarsError),
    #[error("S3 error: {0}")]
    S3(String),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Inventory Manifest
#[derive(Deserialize)]
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
        let response = download(client, file)
            .await
            .map_err(|e| InventoryError::S3(e.to_string()))?;
        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| InventoryError::S3(e.to_string()))?
            .into_bytes();
        Ok(serde_json::from_slice(&bytes)?)
    }
}

/// Inventory Manifest File Entry
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFileEntry {
    pub key: String,
    pub size: u64,
    #[serde(rename = "MD5checksum")]
    pub md5_checksum: String,
}

/// Inventory Stats
#[derive(Serialize)]
pub struct InventoryStats {
    pub total_files: usize,
    pub total_size: i64,
    pub by_prefix: Vec<PrefixStats>,
}

/// Inventory Stats by (top level) prefix
#[derive(Serialize)]
pub struct PrefixStats {
    pub prefix: String,
    pub total_files: u32,
    pub total_size: i64,
}

/// Handles parquet format inventory files from S3
pub struct InventoryProcessor {
    df: DataFrame,
}

impl InventoryProcessor {
    pub fn load(parquet_files: &[&str]) -> Result<Self, InventoryError> {
        let paths: Buffer<PlPath> = parquet_files.iter().map(|f| PlPath::from_str(f)).collect();

        let df = LazyFrame::scan_parquet_files(paths, Default::default())?
            .filter(col(INVENTORY_OBJECT_KEY).str().ends_with(lit("/")).not())
            .collect()?;

        Ok(Self { df })
    }

    pub fn decode_keys(mut self) -> Result<Self, InventoryError> {
        let key_col = self.df.column(INVENTORY_OBJECT_KEY)?.str()?;
        let decoded: StringChunked =
            key_col.apply(|opt_val| opt_val.map(|v| percent_decode_str(v).decode_utf8_lossy()));
        self.df
            .replace(INVENTORY_OBJECT_KEY, decoded.into_series())?;
        Ok(self)
    }

    pub fn write_csv(&mut self, writer: impl Write) -> Result<&mut Self, InventoryError> {
        CsvWriter::new(writer).finish(&mut self.df)?;
        Ok(self)
    }

    pub fn stats(&self) -> Result<InventoryStats, InventoryError> {
        let (total_files, total_size) = self.totals()?;
        let by_prefix = self.prefix_stats()?;
        Ok(InventoryStats {
            total_files,
            total_size,
            by_prefix,
        })
    }

    fn totals(&self) -> Result<(usize, i64), InventoryError> {
        let total_files = self.df.height();
        let total_size = self
            .df
            .column(INVENTORY_SIZE_KEY)?
            .as_materialized_series()
            .sum::<i64>()
            .unwrap_or(0);
        Ok((total_files, total_size))
    }

    fn prefix_stats(&self) -> Result<Vec<PrefixStats>, InventoryError> {
        use polars::prelude::when;

        let by_prefix_df = self
            .df
            .clone()
            .lazy()
            .with_column(
                when(col(INVENTORY_OBJECT_KEY).str().contains(lit("/"), false))
                    .then(
                        col(INVENTORY_OBJECT_KEY)
                            .str()
                            .split(lit("/"))
                            .list()
                            .get(lit(0), false),
                    )
                    .otherwise(lit(""))
                    .alias("prefix"),
            )
            .group_by([col("prefix")])
            .agg([
                col(INVENTORY_OBJECT_KEY).count().alias("total_files"),
                col(INVENTORY_SIZE_KEY).sum().alias("total_size"),
            ])
            .collect()?;

        let prefixes = by_prefix_df.column("prefix")?.str()?;
        let files = by_prefix_df.column("total_files")?.u32()?;
        let sizes = by_prefix_df.column("total_size")?.i64()?;

        let by_prefix: Vec<PrefixStats> = (0..by_prefix_df.height())
            .map(|i| PrefixStats {
                prefix: prefixes.get(i).unwrap_or("").to_string(),
                total_files: files.get(i).unwrap_or(0),
                total_size: sizes.get(i).unwrap_or(0),
            })
            .collect();

        Ok(by_prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::{IntoColumn, NamedFrom, Series};

    fn create_test_df(keys: &[&str], sizes: &[i64]) -> DataFrame {
        let key_col = Series::new("key".into(), keys);
        let size_col = Series::new("size".into(), sizes);
        DataFrame::new(vec![key_col.into_column(), size_col.into_column()]).unwrap()
    }

    fn create_test_processor(keys: &[&str], sizes: &[i64]) -> InventoryProcessor {
        InventoryProcessor {
            df: create_test_df(keys, sizes),
        }
    }

    #[test]
    fn test_deserialize_manifest() {
        let json = include_str!("../../../files/manifest.json");
        let manifest: InventoryManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.source_bucket, "test-stack-private");
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(
            manifest.files[0].key,
            "manifests/test-stack-private/inventory/example.parquet"
        );
    }

    // load tests
    #[test]
    fn test_load_valid_parquet() {
        let processor = InventoryProcessor::load(&["../../files/example.parquet"]).unwrap();
        assert!(processor.df.height() > 0);
    }

    #[test]
    fn test_load_filters_directories() {
        // example.parquet should not contain directory entries (keys ending with /)
        let processor = InventoryProcessor::load(&["../../files/example.parquet"]).unwrap();
        let keys = processor.df.column("key").unwrap().str().unwrap();
        for i in 0..keys.len() {
            let key = keys.get(i).unwrap();
            assert!(!key.ends_with('/'), "Found directory entry: {}", key);
        }
    }

    #[test]
    fn test_load_missing_file() {
        let result = InventoryProcessor::load(&["nonexistent.parquet"]);
        assert!(result.is_err());
    }

    // decode_keys tests
    #[test]
    fn test_decode_keys_space() {
        let processor = create_test_processor(&["my%20file.txt"], &[100]);
        let decoded = processor.decode_keys().unwrap();
        let keys = decoded.df.column("key").unwrap().str().unwrap();
        assert_eq!(keys.get(0).unwrap(), "my file.txt");
    }

    #[test]
    fn test_decode_keys_slash() {
        let processor = create_test_processor(&["path%2Fto%2Ffile.txt"], &[100]);
        let decoded = processor.decode_keys().unwrap();
        let keys = decoded.df.column("key").unwrap().str().unwrap();
        assert_eq!(keys.get(0).unwrap(), "path/to/file.txt");
    }

    #[test]
    fn test_decode_keys_already_decoded() {
        let processor = create_test_processor(&["normal/path/file.txt"], &[100]);
        let decoded = processor.decode_keys().unwrap();
        let keys = decoded.df.column("key").unwrap().str().unwrap();
        assert_eq!(keys.get(0).unwrap(), "normal/path/file.txt");
    }

    #[test]
    fn test_decode_keys_mixed() {
        let processor = create_test_processor(
            &["normal.txt", "with%20space.txt", "path%2Fslash.txt"],
            &[100, 200, 300],
        );
        let decoded = processor.decode_keys().unwrap();
        let keys = decoded.df.column("key").unwrap().str().unwrap();
        assert_eq!(keys.get(0).unwrap(), "normal.txt");
        assert_eq!(keys.get(1).unwrap(), "with space.txt");
        assert_eq!(keys.get(2).unwrap(), "path/slash.txt");
    }

    #[test]
    fn test_decode_keys_special_chars() {
        let processor = create_test_processor(&["file%26name%3Dvalue.txt"], &[100]);
        let decoded = processor.decode_keys().unwrap();
        let keys = decoded.df.column("key").unwrap().str().unwrap();
        assert_eq!(keys.get(0).unwrap(), "file&name=value.txt");
    }

    // write_csv tests
    #[test]
    fn test_write_csv_output() {
        let mut processor = create_test_processor(&["file1.txt", "file2.txt"], &[100, 200]);
        let mut output = Vec::new();
        processor.write_csv(&mut output).unwrap();
        let csv = String::from_utf8(output).unwrap();
        assert!(csv.contains("key,size"));
        assert!(csv.contains("file1.txt,100"));
        assert!(csv.contains("file2.txt,200"));
    }

    #[test]
    fn test_write_csv_empty() {
        let mut processor = create_test_processor(&[], &[]);
        let mut output = Vec::new();
        processor.write_csv(&mut output).unwrap();
        let csv = String::from_utf8(output).unwrap();
        assert!(csv.contains("key,size"));
        assert_eq!(csv.lines().count(), 1); // header only
    }

    // totals tests
    #[test]
    fn test_totals_counts_rows() {
        let processor = create_test_processor(&["a.txt", "b.txt", "c.txt"], &[100, 200, 300]);
        let (total_files, total_size) = processor.totals().unwrap();
        assert_eq!(total_files, 3);
        assert_eq!(total_size, 600);
    }

    #[test]
    fn test_totals_empty() {
        let processor = create_test_processor(&[], &[]);
        let (total_files, total_size) = processor.totals().unwrap();
        assert_eq!(total_files, 0);
        assert_eq!(total_size, 0);
    }

    #[test]
    fn test_totals_with_nulls() {
        let key_col = Series::new("key".into(), &["a.txt", "b.txt"]);
        let size_col = Series::new("size".into(), &[None::<i64>, None::<i64>]);
        let df = DataFrame::new(vec![key_col.into_column(), size_col.into_column()]).unwrap();
        let processor = InventoryProcessor { df };
        let (total_files, total_size) = processor.totals().unwrap();
        assert_eq!(total_files, 2);
        assert_eq!(total_size, 0);
    }

    // prefix_stats tests
    #[test]
    fn test_prefix_stats_groups_correctly() {
        let processor = create_test_processor(
            &[
                "images/a.jpg",
                "images/b.jpg",
                "docs/report.pdf",
                "root.txt",
                "another_root.txt",
            ],
            &[100, 200, 300, 50, 25],
        );
        let stats = processor.prefix_stats().unwrap();

        let images = stats.iter().find(|s| s.prefix == "images").unwrap();
        assert_eq!(images.total_files, 2);
        assert_eq!(images.total_size, 300);

        let docs = stats.iter().find(|s| s.prefix == "docs").unwrap();
        assert_eq!(docs.total_files, 1);
        assert_eq!(docs.total_size, 300);

        // Root-level files grouped under empty prefix
        let root = stats.iter().find(|s| s.prefix.is_empty()).unwrap();
        assert_eq!(root.total_files, 2);
        assert_eq!(root.total_size, 75);
    }

    #[test]
    fn test_prefix_stats_empty() {
        let processor = create_test_processor(&[], &[]);
        let stats = processor.prefix_stats().unwrap();
        assert!(stats.is_empty());
    }

    // integration test
    // TODO: this is tightly coupled to the fixture, may want to generalize
    #[test]
    fn test_full_pipeline() {
        let stats = InventoryProcessor::load(&["../../files/example.parquet"])
            .unwrap()
            .decode_keys()
            .unwrap()
            .stats()
            .unwrap();

        assert_eq!(stats.total_files, 13);
        assert_eq!(stats.total_size, 2191162);

        let find_prefix = |name: &str| stats.by_prefix.iter().find(|p| p.prefix == name);

        // Root-level files grouped under empty prefix
        let root = find_prefix("").expect("root prefix should exist");
        assert_eq!(root.total_files, 6);
        assert_eq!(root.total_size, 1355662);

        let images = find_prefix("images").expect("images prefix should exist");
        assert_eq!(images.total_files, 3);
        assert_eq!(images.total_size, 129000);

        let documents = find_prefix("documents").expect("documents prefix should exist");
        assert_eq!(documents.total_files, 3);
        assert_eq!(documents.total_size, 206500);

        let archive = find_prefix("archive").expect("archive prefix should exist");
        assert_eq!(archive.total_files, 1);
        assert_eq!(archive.total_size, 500000);
    }
}
