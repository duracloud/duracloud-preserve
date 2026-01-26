use duckdb::{Connection, Error as DuckDBError};
use thiserror::Error;

pub fn process(_source_report: &str, _replication_report: &str) {}

#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("DuckDB error: {0}")]
    DuckDB(#[from] DuckDBError),
}

/// Handles csv format checksum report files from S3
#[derive(Debug)]
pub struct ChecksumVerifier {
    conn: Connection,
}

impl ChecksumVerifier {
    pub fn load(source_report: &str, replication_report: &str) -> Result<Self, ChecksumError> {
        let conn = Connection::open_in_memory()?;

        conn.execute_batch(&format!(
            r#"
            CREATE TABLE source AS SELECT * FROM '{source_report}';
            CREATE TABLE replication AS SELECT * FROM '{replication_report}';
            "#
        ))?;

        Ok(Self { conn })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_valid_csv() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let source_count: i64 = verifier
            .conn
            .query_row("SELECT COUNT(*) FROM source", [], |row| row.get(0))
            .unwrap();
        let replication_count: i64 = verifier
            .conn
            .query_row("SELECT COUNT(*) FROM replication", [], |row| row.get(0))
            .unwrap();

        assert_eq!(source_count, 4);
        assert_eq!(replication_count, 4);
    }
}
