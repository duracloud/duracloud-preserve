use std::io::Write;

use duckdb::{Connection, Error as DuckDBError};
use serde::Serialize;
use thiserror::Error;

pub fn process(parquet_files: &[&str]) -> Result<(Vec<u8>, InventoryStats), InventoryError> {
    let processor = InventoryProcessor::load(parquet_files)?;
    let mut csv = Vec::new();
    processor.write_csv(&mut csv)?;
    let stats = processor.stats()?;
    Ok((csv, stats))
}

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("DuckDB error: {0}")]
    DuckDB(#[from] DuckDBError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Inventory Stats
#[derive(Debug, Serialize)]
pub struct InventoryStats {
    pub total_files: usize,
    pub total_size: i64,
    pub by_prefix: Vec<PrefixStats>,
}

/// Inventory Stats by (top level) prefix
#[derive(Debug, Serialize)]
pub struct PrefixStats {
    pub prefix: String,
    pub total_files: u32,
    pub total_size: i64,
}

/// Handles parquet format inventory files from S3
#[derive(Debug)]
pub struct InventoryProcessor {
    conn: Connection,
}

impl InventoryProcessor {
    pub fn load(parquet_files: &[&str]) -> Result<Self, InventoryError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("LOAD parquet;")?;

        let files_list = parquet_files
            .iter()
            .map(|f| format!("'{}'", f))
            .collect::<Vec<_>>()
            .join(", ");

        conn.execute_batch(&format!(
            r#"
            CREATE TABLE inventory AS
            SELECT
                bucket,
                url_decode(key) as key,
                size,
                last_modified_date,
                storage_class,
                replication_status,
                'https://' || bucket || '.s3.amazonaws.com/' || key as url
            FROM read_parquet([{files_list}])
            WHERE NOT key LIKE '%/'
            "#
        ))?;

        Ok(Self { conn })
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

    pub fn totals(&self) -> Result<(usize, i64), InventoryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*), COALESCE(SUM(size), 0) FROM inventory")?;
        let mut rows = stmt.query([])?;
        let row = rows.next()?.expect("COUNT always returns a row");
        let total_files: i64 = row.get(0)?;
        let total_size: i64 = row.get(1)?;
        Ok((total_files as usize, total_size))
    }

    fn prefix_stats(&self) -> Result<Vec<PrefixStats>, InventoryError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                CASE WHEN key LIKE '%/%'
                     THEN split_part(key, '/', 1)
                     ELSE '' END as prefix,
                COUNT(*)::INTEGER as total_files,
                COALESCE(SUM(size), 0) as total_size
            FROM inventory
            GROUP BY prefix
            "#,
        )?;

        let mut rows = stmt.query([])?;
        let mut by_prefix = Vec::new();

        while let Some(row) = rows.next()? {
            by_prefix.push(PrefixStats {
                prefix: row.get(0)?,
                total_files: row.get::<_, i32>(1)? as u32,
                total_size: row.get(2)?,
            });
        }

        Ok(by_prefix)
    }

    pub fn write_csv(&self, writer: impl Write) -> Result<&Self, InventoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT bucket, key, size, last_modified_date::VARCHAR, storage_class, replication_status, url FROM inventory",
        )?;
        let mut rows = stmt.query([])?;

        let mut csv_writer = csv::Writer::from_writer(writer);
        csv_writer.write_record([
            "bucket",
            "key",
            "size",
            "last_modified_date",
            "storage_class",
            "replication_status",
            "url",
        ])?;

        while let Some(row) = rows.next()? {
            let bucket: String = row.get(0)?;
            let key: String = row.get(1)?;
            let size: i64 = row.get(2)?;
            let last_modified: String = row.get::<_, String>(3)?;
            let storage_class: String = row.get(4)?;
            let replication_status: String = row.get(5)?;
            let url: String = row.get(6)?;

            csv_writer.write_record([
                bucket,
                key,
                size.to_string(),
                last_modified,
                storage_class,
                replication_status,
                url,
            ])?;
        }

        csv_writer.flush()?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_processor(rows: &[(&str, &str, i64)]) -> InventoryProcessor {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE inventory (
                bucket VARCHAR,
                key VARCHAR,
                size BIGINT,
                last_modified_date TIMESTAMP WITH TIME ZONE,
                storage_class VARCHAR,
                replication_status VARCHAR,
                url VARCHAR
            )
            "#,
        )
        .unwrap();

        let mut stmt = conn
            .prepare(
                r#"
                INSERT INTO inventory (bucket, key, size, last_modified_date, storage_class, replication_status, url)
                VALUES (?, ?, ?, '2025-01-01 00:00:00+00', 'STANDARD', 'COMPLETED', 'https://' || ? || '.s3.amazonaws.com/' || ?)
                "#,
            )
            .unwrap();

        for (bucket, key, size) in rows {
            stmt.execute(duckdb::params![bucket, key, size, bucket, key])
                .unwrap();
        }

        InventoryProcessor { conn }
    }

    // load tests
    #[test]
    fn test_load_valid_parquet() {
        let processor = InventoryProcessor::load(&["../../files/example.parquet"]).unwrap();
        let (total_files, _) = processor.totals().unwrap();
        assert!(total_files > 0);
    }

    #[test]
    fn test_load_filters_directories() {
        let processor = InventoryProcessor::load(&["../../files/example.parquet"]).unwrap();
        let mut stmt = processor.conn.prepare("SELECT key FROM inventory").unwrap();
        let mut rows = stmt.query([]).unwrap();

        while let Some(row) = rows.next().unwrap() {
            let key: String = row.get(0).unwrap();
            assert!(!key.ends_with('/'), "Found directory entry: {}", key);
        }
    }

    #[test]
    fn test_load_creates_url_column() {
        let processor = InventoryProcessor::load(&["../../files/example.parquet"]).unwrap();
        let mut stmt = processor
            .conn
            .prepare("SELECT bucket, key, url FROM inventory LIMIT 1")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();

        let bucket: String = row.get(0).unwrap();
        let _key: String = row.get(1).unwrap();
        let url: String = row.get(2).unwrap();

        // URL should use original encoded key, not decoded
        assert!(url.starts_with(&format!("https://{}.s3.amazonaws.com/", bucket)));
    }

    #[test]
    fn test_load_decodes_keys() {
        let processor = InventoryProcessor::load(&["../../files/example.parquet"]).unwrap();
        let mut stmt = processor
            .conn
            .prepare("SELECT key FROM inventory WHERE key LIKE '%report%'")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        let key: String = row.get(0).unwrap();
        // The key "documents/my%20report.pdf" should be decoded to "documents/my report.pdf"
        assert_eq!(key, "documents/my report.pdf");
    }

    #[test]
    fn test_load_missing_file() {
        let result = InventoryProcessor::load(&["nonexistent.parquet"]);
        assert!(result.is_err());
    }

    // url_decode tests
    #[test]
    fn test_url_decode_space() {
        let conn = Connection::open_in_memory().unwrap();
        let result: String = conn
            .query_row("SELECT url_decode('my%20file.txt')", [], |row| row.get(0))
            .unwrap();
        assert_eq!(result, "my file.txt");
    }

    #[test]
    fn test_url_decode_slash() {
        let conn = Connection::open_in_memory().unwrap();
        let result: String = conn
            .query_row("SELECT url_decode('path%2Fto%2Ffile.txt')", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(result, "path/to/file.txt");
    }

    #[test]
    fn test_url_decode_already_decoded() {
        let conn = Connection::open_in_memory().unwrap();
        let result: String = conn
            .query_row("SELECT url_decode('normal/path/file.txt')", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(result, "normal/path/file.txt");
    }

    #[test]
    fn test_url_decode_special_chars() {
        let conn = Connection::open_in_memory().unwrap();
        let result: String = conn
            .query_row("SELECT url_decode('file%26name%3Dvalue.txt')", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(result, "file&name=value.txt");
    }

    // write_csv tests
    #[test]
    fn test_write_csv_output() {
        let processor = create_test_processor(&[
            ("mybucket", "file1.txt", 100),
            ("mybucket", "file2.txt", 200),
        ]);
        let mut output = Vec::new();
        processor.write_csv(&mut output).unwrap();
        let csv = String::from_utf8(output).unwrap();

        assert!(
            csv.contains("bucket,key,size,last_modified_date,storage_class,replication_status,url")
        );
        assert!(csv.contains("mybucket,file1.txt,100,"));
        assert!(csv.contains("mybucket,file2.txt,200,"));
        assert!(csv.contains("https://mybucket.s3.amazonaws.com/file1.txt"));
    }

    #[test]
    fn test_write_csv_empty() {
        let processor = create_test_processor(&[]);
        let mut output = Vec::new();
        processor.write_csv(&mut output).unwrap();
        let csv = String::from_utf8(output).unwrap();
        assert!(
            csv.contains("bucket,key,size,last_modified_date,storage_class,replication_status,url")
        );
        assert_eq!(csv.lines().count(), 1); // header only
    }

    // totals tests
    #[test]
    fn test_totals_counts_rows() {
        let processor = create_test_processor(&[
            ("bucket", "a.txt", 100),
            ("bucket", "b.txt", 200),
            ("bucket", "c.txt", 300),
        ]);
        let (total_files, total_size) = processor.totals().unwrap();
        assert_eq!(total_files, 3);
        assert_eq!(total_size, 600);
    }

    #[test]
    fn test_totals_empty() {
        let processor = create_test_processor(&[]);
        let (total_files, total_size) = processor.totals().unwrap();
        assert_eq!(total_files, 0);
        assert_eq!(total_size, 0);
    }

    #[test]
    fn test_totals_with_nulls() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE inventory (
                bucket VARCHAR,
                key VARCHAR,
                size BIGINT,
                last_modified_date TIMESTAMP WITH TIME ZONE,
                storage_class VARCHAR,
                replication_status VARCHAR,
                url VARCHAR
            )
            "#,
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO inventory (key, size) VALUES ('a.txt', NULL), ('b.txt', NULL)",
        )
        .unwrap();
        let processor = InventoryProcessor { conn };
        let (total_files, total_size) = processor.totals().unwrap();
        assert_eq!(total_files, 2);
        assert_eq!(total_size, 0);
    }

    // prefix_stats tests
    #[test]
    fn test_prefix_stats_groups_correctly() {
        let processor = create_test_processor(&[
            ("bucket", "images/a.jpg", 100),
            ("bucket", "images/b.jpg", 200),
            ("bucket", "docs/report.pdf", 300),
            ("bucket", "root.txt", 50),
            ("bucket", "another_root.txt", 25),
        ]);
        let stats = processor.prefix_stats().unwrap();

        let images = stats.iter().find(|s| s.prefix == "images").unwrap();
        assert_eq!(images.total_files, 2);
        assert_eq!(images.total_size, 300);

        let docs = stats.iter().find(|s| s.prefix == "docs").unwrap();
        assert_eq!(docs.total_files, 1);
        assert_eq!(docs.total_size, 300);

        let root = stats.iter().find(|s| s.prefix.is_empty()).unwrap();
        assert_eq!(root.total_files, 2);
        assert_eq!(root.total_size, 75);
    }

    #[test]
    fn test_prefix_stats_empty() {
        let processor = create_test_processor(&[]);
        let stats = processor.prefix_stats().unwrap();
        assert!(stats.is_empty());
    }

    // integration test
    #[test]
    fn test_full_pipeline() {
        let stats = InventoryProcessor::load(&["../../files/example.parquet"])
            .unwrap()
            .stats()
            .unwrap();

        assert_eq!(stats.total_files, 13);
        assert_eq!(stats.total_size, 2191162);

        let find_prefix = |name: &str| stats.by_prefix.iter().find(|p| p.prefix == name);

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

    #[test]
    fn test_full_pipeline_csv_output() {
        let mut output = Vec::new();
        let _stats = InventoryProcessor::load(&["../../files/example.parquet"])
            .unwrap()
            .write_csv(&mut output)
            .unwrap()
            .stats()
            .unwrap();

        let csv = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = csv.lines().collect();

        // Header + 13 data rows
        assert_eq!(lines.len(), 14);

        assert_eq!(
            lines[0],
            "bucket,key,size,last_modified_date,storage_class,replication_status,url"
        );

        assert!(csv.contains("https://test-stack-private.s3.amazonaws.com/"));
    }
}
