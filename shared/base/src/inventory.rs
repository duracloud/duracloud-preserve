use std::collections::BTreeMap;
use std::io::Write;

use duckdb::Connection;

use crate::{
    errors::ProcessingError,
    safe_join,
    stats::{InventoryStats, PrefixStats},
};

pub fn process(
    parquet_files: &[impl AsRef<str>],
) -> Result<(Vec<u8>, InventoryStats), ProcessingError> {
    let processor = InventoryProcessor::load(parquet_files)?;
    let mut csv = Vec::new();
    let stats = processor.write_csv_and_stats(&mut csv)?;
    Ok((csv, stats))
}

/// Handles parquet format inventory files from S3
#[derive(Debug)]
pub struct InventoryProcessor {
    conn: Connection,
}

impl InventoryProcessor {
    pub fn load(parquet_files: &[impl AsRef<str>]) -> Result<Self, ProcessingError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("LOAD parquet;")?;

        let files = safe_join(parquet_files);

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
            FROM read_parquet([{files}])
            WHERE NOT key LIKE '%/'
            "#
        ))?;

        Ok(Self { conn })
    }

    fn write_csv_and_stats(&self, writer: impl Write) -> Result<InventoryStats, ProcessingError> {
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

        let mut total_files = 0_u64;
        let mut total_size = 0_u64;
        let mut prefix_totals: BTreeMap<String, (u64, u64)> = BTreeMap::new();

        while let Some(row) = rows.next()? {
            let bucket: String = row.get(0)?;
            let key: String = row.get(1)?;
            let size: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
            let last_modified: String = row.get(3)?;
            let storage_class: String = row.get(4)?;
            let replication_status: String = row.get(5)?;
            let url: String = row.get(6)?;

            csv_writer.write_record([
                &bucket,
                &key,
                &size.to_string(),
                &last_modified,
                &storage_class,
                &replication_status,
                &url,
            ])?;

            total_files += 1;

            let size_u64 = u64::try_from(size).unwrap_or(0);
            total_size += size_u64;

            let prefix = key
                .split_once('/')
                .map_or_else(String::new, |(p, _)| p.to_string());
            let (files, bytes) = prefix_totals.entry(prefix).or_insert((0, 0));
            *files += 1;
            *bytes += size_u64;
        }

        csv_writer.flush()?;

        let by_prefix = prefix_totals
            .into_iter()
            .map(|(prefix, (total_files, total_size))| {
                (
                    prefix,
                    PrefixStats {
                        total_files,
                        total_size,
                    },
                )
            })
            .collect();

        Ok(InventoryStats {
            total_files,
            total_size,
            by_prefix,
        })
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

    #[test]
    fn test_full_pipeline() {
        let (_csv, stats) = process(&["../../files/example.parquet"]).unwrap();

        assert_eq!(stats.total_files, 13);
        assert_eq!(stats.total_size, 2191162);

        let by_prefix = &stats.by_prefix;

        let root = by_prefix.get("").expect("root prefix should exist");
        assert_eq!(root.total_files, 6);
        assert_eq!(root.total_size, 1355662);

        let images = by_prefix.get("images").expect("images prefix should exist");
        assert_eq!(images.total_files, 3);
        assert_eq!(images.total_size, 129000);

        let documents = by_prefix
            .get("documents")
            .expect("documents prefix should exist");
        assert_eq!(documents.total_files, 3);
        assert_eq!(documents.total_size, 206500);

        let archive = by_prefix
            .get("archive")
            .expect("archive prefix should exist");
        assert_eq!(archive.total_files, 1);
        assert_eq!(archive.total_size, 500000);
    }

    #[test]
    fn test_full_pipeline_csv_output() {
        let (output, _stats) = process(&["../../files/example.parquet"]).unwrap();

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
    fn test_load_missing_file() {
        let result = InventoryProcessor::load(&["nonexistent.parquet"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_valid_parquet() {
        let processor = InventoryProcessor::load(&["../../files/example.parquet"]).unwrap();
        let total_files: u64 = processor
            .conn
            .query_row("SELECT COUNT(*)::UBIGINT FROM inventory", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(total_files > 0);
    }

    #[test]
    fn test_prefix_stats_empty() {
        let processor = create_test_processor(&[]);
        let stats = processor.write_csv_and_stats(std::io::sink()).unwrap();
        assert!(stats.by_prefix.is_empty());
    }

    #[test]
    fn test_prefix_stats_groups_correctly() {
        let processor = create_test_processor(&[
            ("bucket", "images/a.jpg", 100),
            ("bucket", "images/b.jpg", 200),
            ("bucket", "docs/report.pdf", 300),
            ("bucket", "root.txt", 50),
            ("bucket", "another_root.txt", 25),
        ]);
        let stats = processor.write_csv_and_stats(std::io::sink()).unwrap();
        let by_prefix = &stats.by_prefix;

        let images = by_prefix.get("images").unwrap();
        assert_eq!(images.total_files, 2);
        assert_eq!(images.total_size, 300);

        let docs = by_prefix.get("docs").unwrap();
        assert_eq!(docs.total_files, 1);
        assert_eq!(docs.total_size, 300);

        let root = by_prefix.get("").unwrap();
        assert_eq!(root.total_files, 2);
        assert_eq!(root.total_size, 75);
    }

    #[test]
    fn test_process_returns_matching_csv_and_stats() {
        let (csv, stats) = process(&["../../files/example.parquet"]).unwrap();

        assert_eq!(stats.total_files, 13);
        assert_eq!(stats.total_size, 2_191_162);

        let csv = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 14);
        assert_eq!(
            lines[0],
            "bucket,key,size,last_modified_date,storage_class,replication_status,url"
        );

        let root = stats.by_prefix.get("").expect("root prefix should exist");
        assert_eq!(root.total_files, 6);
        assert_eq!(root.total_size, 1_355_662);
    }

    #[test]
    fn test_totals_counts_rows() {
        let processor = create_test_processor(&[
            ("bucket", "a.txt", 100),
            ("bucket", "b.txt", 200),
            ("bucket", "c.txt", 300),
        ]);
        let stats = processor.write_csv_and_stats(std::io::sink()).unwrap();
        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.total_size, 600);
    }

    #[test]
    fn test_totals_empty() {
        let processor = create_test_processor(&[]);
        let stats = processor.write_csv_and_stats(std::io::sink()).unwrap();
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.total_size, 0);
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
            r#"
            INSERT INTO inventory (bucket, key, size, last_modified_date, storage_class, replication_status, url)
            VALUES
              ('bucket', 'a.txt', NULL, '2025-01-01 00:00:00+00', 'STANDARD', 'COMPLETED', 'https://bucket.s3.amazonaws.com/a.txt'),
              ('bucket', 'b.txt', NULL, '2025-01-01 00:00:00+00', 'STANDARD', 'COMPLETED', 'https://bucket.s3.amazonaws.com/b.txt')
            "#,
        )
        .unwrap();
        let processor = InventoryProcessor { conn };
        let stats = processor.write_csv_and_stats(std::io::sink()).unwrap();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.total_size, 0);
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
    fn test_url_decode_space() {
        let conn = Connection::open_in_memory().unwrap();
        let result: String = conn
            .query_row("SELECT url_decode('my%20file.txt')", [], |row| row.get(0))
            .unwrap();
        assert_eq!(result, "my file.txt");
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

    #[test]
    fn test_write_csv_empty() {
        let processor = create_test_processor(&[]);
        let mut output = Vec::new();
        processor.write_csv_and_stats(&mut output).unwrap();
        let csv = String::from_utf8(output).unwrap();
        assert!(
            csv.contains("bucket,key,size,last_modified_date,storage_class,replication_status,url")
        );
        assert_eq!(csv.lines().count(), 1); // header only
    }

    #[test]
    fn test_write_csv_output() {
        let processor = create_test_processor(&[
            ("mybucket", "file1.txt", 100),
            ("mybucket", "file2.txt", 200),
        ]);
        let mut output = Vec::new();
        processor.write_csv_and_stats(&mut output).unwrap();
        let csv = String::from_utf8(output).unwrap();

        assert!(
            csv.contains("bucket,key,size,last_modified_date,storage_class,replication_status,url")
        );
        assert!(csv.contains("mybucket,file1.txt,100,"));
        assert!(csv.contains("mybucket,file2.txt,200,"));
        assert!(csv.contains("https://mybucket.s3.amazonaws.com/file1.txt"));
    }
}
