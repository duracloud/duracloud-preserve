use std::io::Write;

use duckdb::{Connection, Error as DuckDBError};
use serde::Serialize;
use thiserror::Error;

pub fn process(
    source_report: &str,
    replication_report: &str,
) -> Result<(Vec<u8>, VerificationStats), ChecksumError> {
    let verifier = ChecksumVerifier::load(source_report, replication_report)?;
    let mut csv = Vec::new();
    verifier.write_csv(&mut csv)?;
    let stats = verifier.stats()?;
    Ok((csv, stats))
}

#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("DuckDB error: {0}")]
    DuckDB(#[from] DuckDBError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Verification status for an object
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    Ok,
    Mismatch,
    MissingReplica,
    MissingSource,
    FailedSource,
    FailedReplication,
}

impl VerificationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Mismatch => "mismatch",
            Self::MissingReplica => "missing_replica",
            Self::MissingSource => "missing_source",
            Self::FailedSource => "failed_source",
            Self::FailedReplication => "failed_replication",
        }
    }
}

/// Result of verifying a single object
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub bucket: String,
    pub key: String,
    pub version_id: String,
    pub status: VerificationStatus,
    pub checksum_algorithm: String,
    pub checksum_source: String,
    pub checksum_replication: String,
}

/// An object missing from one side of the replication
#[derive(Debug, Clone)]
pub struct MissingObject {
    pub bucket: String,
    pub key: String,
    pub version_id: String,
}

/// A failed checksum task
#[derive(Debug, Clone)]
pub struct FailedTask {
    pub bucket: String,
    pub key: String,
    pub version_id: String,
    pub error_code: String,
    pub http_status_code: String,
}

/// Summary statistics for verification
#[derive(Debug, Serialize)]
pub struct VerificationStats {
    pub total_objects: usize,
    pub matches: usize,
    pub mismatches: usize,
    pub missing_replica: usize,
    pub missing_source: usize,
    pub failed_source: usize,
    pub failed_replication: usize,
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
            CREATE TABLE source AS
            SELECT
                column0 AS bucket,
                column1 AS key,
                column2 AS version_id,
                column3 AS task_status,
                column4 AS error_code,
                column5 AS http_status_code,
                column6 AS result_message,
                json_extract_string(column6, '$.checksum_hex') AS checksum,
                json_extract_string(column6, '$.checksumAlgorithm') AS checksum_algorithm
            FROM '{source_report}';

            CREATE TABLE replication AS
            SELECT
                column0 AS bucket,
                column1 AS key,
                column2 AS version_id,
                column3 AS task_status,
                column4 AS error_code,
                column5 AS http_status_code,
                column6 AS result_message,
                json_extract_string(column6, '$.checksum_hex') AS checksum,
                json_extract_string(column6, '$.checksumAlgorithm') AS checksum_algorithm
            FROM '{replication_report}';
            "#
        ))?;

        Ok(Self { conn })
    }

    /// Find all objects where checksums match
    pub fn find_matches(&self) -> Result<Vec<VerificationResult>, ChecksumError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                s.bucket,
                s.key,
                s.version_id,
                s.checksum_algorithm,
                s.checksum
            FROM source s
            JOIN replication r
                ON s.key = r.key
                AND s.version_id = r.version_id
            WHERE s.task_status = 'succeeded'
                AND r.task_status = 'succeeded'
                AND s.checksum = r.checksum
            "#,
        )?;

        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            results.push(VerificationResult {
                bucket: row.get(0)?,
                key: row.get(1)?,
                version_id: row.get(2)?,
                status: VerificationStatus::Ok,
                checksum_algorithm: row.get(3)?,
                checksum_source: row.get(4)?,
                checksum_replication: String::new(),
            });
        }

        Ok(results)
    }

    /// Find all objects where checksums do not match
    pub fn find_mismatches(&self) -> Result<Vec<VerificationResult>, ChecksumError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                s.bucket,
                s.key,
                s.version_id,
                s.checksum_algorithm,
                s.checksum,
                r.checksum
            FROM source s
            JOIN replication r
                ON s.key = r.key
                AND s.version_id = r.version_id
            WHERE s.task_status = 'succeeded'
                AND r.task_status = 'succeeded'
                AND s.checksum != r.checksum
            "#,
        )?;

        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            results.push(VerificationResult {
                bucket: row.get(0)?,
                key: row.get(1)?,
                version_id: row.get(2)?,
                status: VerificationStatus::Mismatch,
                checksum_algorithm: row.get(3)?,
                checksum_source: row.get(4)?,
                checksum_replication: row.get(5)?,
            });
        }

        Ok(results)
    }

    /// Find objects that exist in source but not in replication
    pub fn objects_only_in_source(&self) -> Result<Vec<MissingObject>, ChecksumError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT s.bucket, s.key, s.version_id
            FROM source s
            LEFT JOIN replication r
                ON s.key = r.key
                AND s.version_id = r.version_id
            WHERE r.key IS NULL
                AND s.task_status = 'succeeded'
            "#,
        )?;

        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            results.push(MissingObject {
                bucket: row.get(0)?,
                key: row.get(1)?,
                version_id: row.get(2)?,
            });
        }

        Ok(results)
    }

    /// Find objects that exist in replication but not in source
    pub fn objects_only_in_replication(&self) -> Result<Vec<MissingObject>, ChecksumError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT r.bucket, r.key, r.version_id
            FROM replication r
            LEFT JOIN source s
                ON r.key = s.key
                AND r.version_id = s.version_id
            WHERE s.key IS NULL
                AND r.task_status = 'succeeded'
            "#,
        )?;

        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            results.push(MissingObject {
                bucket: row.get(0)?,
                key: row.get(1)?,
                version_id: row.get(2)?,
            });
        }

        Ok(results)
    }

    /// Find failed tasks in source
    pub fn failed_in_source(&self) -> Result<Vec<FailedTask>, ChecksumError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT bucket, key, version_id, error_code, http_status_code
            FROM source
            WHERE task_status = 'failed'
            "#,
        )?;

        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            results.push(FailedTask {
                bucket: row.get(0)?,
                key: row.get(1)?,
                version_id: row.get(2)?,
                error_code: row.get(3)?,
                http_status_code: row.get(4)?,
            });
        }

        Ok(results)
    }

    /// Find failed tasks in replication
    pub fn failed_in_replication(&self) -> Result<Vec<FailedTask>, ChecksumError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT bucket, key, version_id, error_code, http_status_code
            FROM replication
            WHERE task_status = 'failed'
            "#,
        )?;

        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            results.push(FailedTask {
                bucket: row.get(0)?,
                key: row.get(1)?,
                version_id: row.get(2)?,
                error_code: row.get(3)?,
                http_status_code: row.get(4)?,
            });
        }

        Ok(results)
    }

    /// Get summary statistics
    pub fn stats(&self) -> Result<VerificationStats, ChecksumError> {
        let matches = self.find_matches()?.len();
        let mismatches = self.find_mismatches()?.len();
        let missing_replica = self.objects_only_in_source()?.len();
        let missing_source = self.objects_only_in_replication()?.len();
        let failed_source = self.failed_in_source()?.len();
        let failed_replication = self.failed_in_replication()?.len();

        Ok(VerificationStats {
            total_objects: matches
                + mismatches
                + missing_replica
                + missing_source
                + failed_source
                + failed_replication,
            matches,
            mismatches,
            missing_replica,
            missing_source,
            failed_source,
            failed_replication,
        })
    }

    /// Write verification results to CSV
    pub fn write_csv(&self, writer: impl Write) -> Result<&Self, ChecksumError> {
        let mut csv_writer = csv::Writer::from_writer(writer);
        csv_writer.write_record([
            "bucket",
            "key",
            "version_id",
            "status",
            "checksum_algorithm",
            "checksum_source",
            "checksum_replication",
        ])?;

        for result in self.find_matches()? {
            csv_writer.write_record([
                &result.bucket,
                &result.key,
                &result.version_id,
                result.status.as_str(),
                &result.checksum_algorithm,
                &result.checksum_source,
                &result.checksum_replication,
            ])?;
        }

        for result in self.find_mismatches()? {
            csv_writer.write_record([
                &result.bucket,
                &result.key,
                &result.version_id,
                result.status.as_str(),
                &result.checksum_algorithm,
                &result.checksum_source,
                &result.checksum_replication,
            ])?;
        }

        for obj in self.objects_only_in_source()? {
            csv_writer.write_record([
                &obj.bucket,
                &obj.key,
                &obj.version_id,
                VerificationStatus::MissingReplica.as_str(),
                "",
                "",
                "",
            ])?;
        }

        for obj in self.objects_only_in_replication()? {
            csv_writer.write_record([
                &obj.bucket,
                &obj.key,
                &obj.version_id,
                VerificationStatus::MissingSource.as_str(),
                "",
                "",
                "",
            ])?;
        }

        for task in self.failed_in_source()? {
            csv_writer.write_record([
                &task.bucket,
                &task.key,
                &task.version_id,
                VerificationStatus::FailedSource.as_str(),
                "",
                "",
                "",
            ])?;
        }

        for task in self.failed_in_replication()? {
            csv_writer.write_record([
                &task.bucket,
                &task.key,
                &task.version_id,
                VerificationStatus::FailedReplication.as_str(),
                "",
                "",
                "",
            ])?;
        }

        csv_writer.flush()?;
        Ok(self)
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

    #[test]
    fn test_load_extracts_checksum() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let checksum: String = verifier
            .conn
            .query_row(
                "SELECT checksum FROM source WHERE key = 'wasteland01elio.pdf'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(
            checksum,
            "9769072B18003829D11657060DC98DCFB5C96D50C197BF40EB40D92E2D2D7869"
        );
    }

    #[test]
    fn test_load_extracts_algorithm() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let algorithm: String = verifier
            .conn
            .query_row(
                "SELECT checksum_algorithm FROM source WHERE key = 'wasteland01elio.pdf'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(algorithm, "SHA256");
    }

    #[test]
    fn test_find_matches() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let matches = verifier.find_matches().unwrap();
        assert_eq!(matches.len(), 4);

        for result in &matches {
            assert_eq!(result.status, VerificationStatus::Ok);
            assert!(!result.checksum_source.is_empty());
            assert!(result.checksum_replication.is_empty());
        }
    }

    #[test]
    fn test_find_mismatches_empty_when_all_match() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let mismatches = verifier.find_mismatches().unwrap();
        assert!(mismatches.is_empty());
    }

    #[test]
    fn test_objects_only_in_source_empty_when_all_present() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let missing = verifier.objects_only_in_source().unwrap();
        assert!(missing.is_empty());
    }

    #[test]
    fn test_objects_only_in_replication_empty_when_all_present() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let missing = verifier.objects_only_in_replication().unwrap();
        assert!(missing.is_empty());
    }

    #[test]
    fn test_stats() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.total_objects, 4);
        assert_eq!(stats.matches, 4);
        assert_eq!(stats.mismatches, 0);
        assert_eq!(stats.missing_replica, 0);
        assert_eq!(stats.missing_source, 0);
        assert_eq!(stats.failed_source, 0);
        assert_eq!(stats.failed_replication, 0);
    }

    #[test]
    fn test_write_csv() {
        let verifier = ChecksumVerifier::load(
            "../../files/checksum-source.csv",
            "../../files/checksum-replication.csv",
        )
        .unwrap();

        let mut output = Vec::new();
        verifier.write_csv(&mut output).unwrap();

        let csv = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = csv.lines().collect();

        // Header + 4 data rows
        assert_eq!(lines.len(), 5);
        assert_eq!(
            lines[0],
            "bucket,key,version_id,status,checksum_algorithm,checksum_source,checksum_replication"
        );

        // All should be "ok" status
        for line in &lines[1..] {
            assert!(line.contains(",ok,"));
            assert!(line.contains(",SHA256,"));
        }
    }
}
