use std::{error::Error, io::Write};

use percent_encoding::percent_decode_str;
use polars::{
    frame::DataFrame,
    prelude::{
        ChunkApply, CsvWriter, IntoLazy, LazyFrame, PlPath, SerWriter, StringChunked, col, lit,
    },
    series::IntoSeries,
};
use polars_arrow::buffer::Buffer;
use serde::Deserialize;

const INVENTORY_OBJECT_KEY: &str = "Key";
const INVENTORY_SIZE_KEY: &str = "Size";

pub fn process(
    mut csv_file: impl Write,
    parquet_files: &[&str],
) -> Result<InventoryStats, Box<dyn Error>> {
    InventoryProcessor::load(&parquet_files)?
        .decode_keys()?
        .write_csv(&mut csv_file)?
        .stats()
}

/// Inventory Manifest
#[derive(Deserialize)]
pub struct InventoryManifest {
    pub sourceBucket: String,
    pub destinationBucket: String,
    pub version: String,
    pub creationTimestamp: String,
    pub fileFormat: String,
    pub fileSchema: String,
    pub files: Vec<InventoryFileEntry>,
}

/// Inventory Manifest File Entry
#[derive(Deserialize)]
pub struct InventoryFileEntry {
    pub key: String,
    pub size: u64,
    pub MD5checksum: String,
}

/// Inventory Stats
pub struct InventoryStats {
    pub total_files: usize,
    pub total_size: i64,
    pub by_prefix: Vec<PrefixStats>,
}

/// Inventory Stats by (top level) prefix
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
    pub fn load(parquet_files: &[&str]) -> Result<Self, Box<dyn Error>> {
        let paths: Buffer<PlPath> = parquet_files.iter().map(|f| PlPath::from_str(f)).collect();

        let df = LazyFrame::scan_parquet_files(paths, Default::default())?
            .filter(col(INVENTORY_OBJECT_KEY).str().ends_with(lit("/")).not())
            .collect()?;

        Ok(Self { df })
    }

    pub fn decode_keys(mut self) -> Result<Self, Box<dyn Error>> {
        let key_col = self.df.column(INVENTORY_OBJECT_KEY)?.str()?;
        let decoded: StringChunked =
            key_col.apply(|opt_val| opt_val.map(|v| percent_decode_str(v).decode_utf8_lossy()));
        self.df
            .replace(INVENTORY_OBJECT_KEY, decoded.into_series())?;
        Ok(self)
    }

    pub fn write_csv(&mut self, writer: impl Write) -> Result<&mut Self, Box<dyn Error>> {
        CsvWriter::new(writer).finish(&mut self.df)?;
        Ok(self)
    }

    pub fn totals(&self) -> Result<(usize, i64), Box<dyn Error>> {
        let total_files = self.df.height();
        let total_size = self
            .df
            .column(INVENTORY_SIZE_KEY)?
            .as_materialized_series()
            .sum::<i64>()
            .unwrap_or(0);
        Ok((total_files, total_size))
    }

    pub fn prefix_stats(&self) -> Result<Vec<PrefixStats>, Box<dyn Error>> {
        let by_prefix_df = self
            .df
            .clone()
            .lazy()
            .with_column(
                col(INVENTORY_OBJECT_KEY)
                    .str()
                    .split(lit("/"))
                    .list()
                    .get(lit(0), false)
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

    pub fn stats(&self) -> Result<InventoryStats, Box<dyn Error>> {
        let (total_files, total_size) = self.totals()?;
        let by_prefix = self.prefix_stats()?;
        Ok(InventoryStats {
            total_files,
            total_size,
            by_prefix,
        })
    }
}
