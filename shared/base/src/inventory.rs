use std::collections::BTreeMap;
use std::path::Path;

use duckdb::Connection;

use crate::{
    errors::ProcessingError,
    safe_join,
    stats::{InventoryStats, PrefixStats},
};

pub fn process(
    parquet_files: &[impl AsRef<str>],
    output_csv: &Path,
) -> Result<InventoryStats, ProcessingError> {
    let processor = InventoryProcessor::load(parquet_files)?;
    processor.write_csv(output_csv)?;
    processor.compute_stats()
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
            CREATE VIEW inventory AS
            SELECT
                bucket,
                key,
                size,
                last_modified_date,
                storage_class,
                replication_status
            FROM read_parquet([{files}])
            WHERE NOT key LIKE '%/'
            "#
        ))?;

        Ok(Self { conn })
    }

    /// Stream the report straight from DuckDB to a CSV file
    fn write_csv(&self, output: &Path) -> Result<(), ProcessingError> {
        if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
            self.conn.execute_batch(&format!(
                "SET temp_directory = {};",
                safe_join(&[dir.to_string_lossy()])
            ))?;
        }

        self.conn.execute_batch(&format!(
            r#"
            COPY (
                SELECT
                    bucket,
                    key,
                    coalesce(size, 0) AS size,
                    last_modified_date::VARCHAR AS last_modified_date,
                    storage_class,
                    replication_status
                FROM inventory
            ) TO {} (FORMAT CSV, HEADER)
            "#,
            safe_join(&[output.to_string_lossy()])
        ))?;

        Ok(())
    }

    /// Tally totals per top-level prefix with a single aggregate query
    fn compute_stats(&self) -> Result<InventoryStats, ProcessingError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                CASE WHEN strpos(key, '/') > 0 THEN split_part(key, '/', 1) ELSE '' END AS prefix,
                count(*)::UBIGINT AS total_files,
                sum(coalesce(size, 0))::UBIGINT AS total_size
            FROM inventory
            GROUP BY prefix
            "#,
        )?;
        let mut rows = stmt.query([])?;

        let mut total_files = 0_u64;
        let mut total_size = 0_u64;
        let mut by_prefix: BTreeMap<String, PrefixStats> = BTreeMap::new();

        while let Some(row) = rows.next()? {
            let prefix: String = row.get(0)?;
            let files: u64 = row.get(1)?;
            let size: u64 = row.get(2)?;

            total_files += files;
            total_size += size;
            by_prefix.insert(
                prefix,
                PrefixStats {
                    total_files: files,
                    total_size: size,
                },
            );
        }

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
                replication_status VARCHAR
            )
            "#,
        )
        .unwrap();

        let mut stmt = conn
            .prepare(
                r#"
                INSERT INTO inventory (bucket, key, size, last_modified_date, storage_class, replication_status)
                VALUES (?, ?, ?, '2025-01-01 00:00:00+00', 'STANDARD', 'COMPLETED')
                "#,
            )
            .unwrap();

        for (bucket, key, size) in rows {
            stmt.execute(duckdb::params![bucket, key, size]).unwrap();
        }

        InventoryProcessor { conn }
    }

    /// Run `process` against a temp output file, returning the CSV text and stats.
    fn process_to_temp(parquet_files: &[&str]) -> (String, InventoryStats) {
        let temp_dir = tempfile::tempdir().unwrap();
        let output = temp_dir.path().join("inventory-report.csv");
        let stats = process(parquet_files, &output).unwrap();
        let csv = std::fs::read_to_string(&output).unwrap();
        (csv, stats)
    }

    /// Run `write_csv` against a temp output file, returning the CSV text.
    fn write_csv_to_temp(processor: &InventoryProcessor) -> String {
        let temp_dir = tempfile::tempdir().unwrap();
        let output = temp_dir.path().join("inventory-report.csv");
        processor.write_csv(&output).unwrap();
        std::fs::read_to_string(&output).unwrap()
    }

    #[test]
    fn test_full_pipeline() {
        let (_csv, stats) = process_to_temp(&["../../files/example.parquet"]);

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
        let (csv, _stats) = process_to_temp(&["../../files/example.parquet"]);

        let lines: Vec<&str> = csv.lines().collect();

        // Header + 13 data rows
        assert_eq!(lines.len(), 14);

        assert_eq!(
            lines[0],
            "bucket,key,size,last_modified_date,storage_class,replication_status"
        );
    }

    #[test]
    fn test_load_preserves_keys_verbatim() {
        let processor = InventoryProcessor::load(&["../../files/example.parquet"]).unwrap();

        let found: i64 = processor
            .conn
            .query_row(
                "SELECT count(*) FROM inventory WHERE key = 'documents/my report.pdf'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "literal-space key should be read verbatim");

        // No percent-encoded variant should leak through.
        let encoded: i64 = processor
            .conn
            .query_row(
                r"SELECT count(*) FROM inventory WHERE key LIKE '%\%20%' ESCAPE '\'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(encoded, 0, "keys should not be percent-encoded");
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
        let stats = processor.compute_stats().unwrap();
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
        let stats = processor.compute_stats().unwrap();
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
        let (csv, stats) = process_to_temp(&["../../files/example.parquet"]);

        assert_eq!(stats.total_files, 13);
        assert_eq!(stats.total_size, 2_191_162);

        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 14);
        assert_eq!(
            lines[0],
            "bucket,key,size,last_modified_date,storage_class,replication_status"
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
        let stats = processor.compute_stats().unwrap();
        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.total_size, 600);
    }

    #[test]
    fn test_totals_empty() {
        let processor = create_test_processor(&[]);
        let stats = processor.compute_stats().unwrap();
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
                replication_status VARCHAR
            )
            "#,
        )
        .unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO inventory (bucket, key, size, last_modified_date, storage_class, replication_status)
            VALUES
              ('bucket', 'a.txt', NULL, '2025-01-01 00:00:00+00', 'STANDARD', 'COMPLETED'),
              ('bucket', 'b.txt', NULL, '2025-01-01 00:00:00+00', 'STANDARD', 'COMPLETED')
            "#,
        )
        .unwrap();
        let processor = InventoryProcessor { conn };
        let stats = processor.compute_stats().unwrap();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.total_size, 0);

        let csv = write_csv_to_temp(&processor);
        assert!(
            csv.contains("bucket,a.txt,0,"),
            "NULL size should render as 0: {csv}"
        );
    }

    #[test]
    fn test_write_csv_empty() {
        let processor = create_test_processor(&[]);
        let csv = write_csv_to_temp(&processor);
        assert!(
            csv.contains("bucket,key,size,last_modified_date,storage_class,replication_status")
        );
        assert_eq!(csv.lines().count(), 1); // header only
    }

    #[test]
    fn test_write_csv_output() {
        let processor = create_test_processor(&[
            ("mybucket", "file1.txt", 100),
            ("mybucket", "file2.txt", 200),
        ]);
        let csv = write_csv_to_temp(&processor);

        assert!(
            csv.contains("bucket,key,size,last_modified_date,storage_class,replication_status")
        );
        assert!(csv.contains("mybucket,file1.txt,100,"));
        assert!(csv.contains("mybucket,file2.txt,200,"));
    }
}
