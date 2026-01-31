use std::fmt;
use std::io::Write;

use duckdb::{Connection, Error as DuckDBError};
use serde::Serialize;
use thiserror::Error;

pub fn process(
    source_reports: &[&str],
    replication_reports: &[&str],
) -> Result<(Vec<u8>, VerificationStats), ChecksumError> {
    let verifier = ChecksumVerifier::load(source_reports, replication_reports)?;
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

impl fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Ok => "ok",
            Self::Mismatch => "mismatch",
            Self::MissingReplica => "missing_replica",
            Self::MissingSource => "missing_source",
            Self::FailedSource => "failed_source",
            Self::FailedReplication => "failed_replication",
        };
        write!(f, "{}", s)
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
    pub fn load(
        source_reports: &[&str],
        replication_reports: &[&str],
    ) -> Result<Self, ChecksumError> {
        let conn = Connection::open_in_memory()?;

        let sources = source_reports
            .iter()
            .map(|p| format!("'{p}'"))
            .collect::<Vec<_>>()
            .join(", ");

        let repls = replication_reports
            .iter()
            .map(|p| format!("'{p}'"))
            .collect::<Vec<_>>()
            .join(", ");

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
            FROM read_csv([{sources}]);

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
            FROM read_csv([{repls}]);
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
                &result.status.to_string(),
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
                &result.status.to_string(),
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
                &VerificationStatus::MissingReplica.to_string(),
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
                &VerificationStatus::MissingSource.to_string(),
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
                &VerificationStatus::FailedSource.to_string(),
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
                &VerificationStatus::FailedReplication.to_string(),
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
    use duckdb::Connection;

    // Test row with all fields
    struct TestRow {
        bucket: &'static str,
        key: &'static str,
        version_id: &'static str,
        task_status: &'static str,
        error_code: &'static str,
        http_status_code: &'static str,
        checksum: &'static str,
        checksum_algorithm: &'static str,
    }

    // Helper for succeeded rows (common case)
    fn row(key: &'static str, version_id: &'static str, checksum: &'static str) -> TestRow {
        TestRow {
            bucket: "test-bucket",
            key,
            version_id,
            task_status: "succeeded",
            error_code: "",
            http_status_code: "200",
            checksum,
            checksum_algorithm: "SHA256",
        }
    }

    // Helper for failed rows
    fn failed_row(key: &'static str, version_id: &'static str) -> TestRow {
        TestRow {
            bucket: "test-bucket",
            key,
            version_id,
            task_status: "failed",
            error_code: "InternalError",
            http_status_code: "500",
            checksum: "",
            checksum_algorithm: "",
        }
    }

    fn create_test_verifier(
        source_rows: &[TestRow],
        replication_rows: &[TestRow],
    ) -> ChecksumVerifier {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE source (
                bucket VARCHAR,
                key VARCHAR,
                version_id VARCHAR,
                task_status VARCHAR,
                error_code VARCHAR,
                http_status_code VARCHAR,
                result_message VARCHAR,
                checksum VARCHAR,
                checksum_algorithm VARCHAR
            );
            CREATE TABLE replication (
                bucket VARCHAR,
                key VARCHAR,
                version_id VARCHAR,
                task_status VARCHAR,
                error_code VARCHAR,
                http_status_code VARCHAR,
                result_message VARCHAR,
                checksum VARCHAR,
                checksum_algorithm VARCHAR
            );
            "#,
        )
        .unwrap();

        let mut source_stmt = conn
            .prepare(
                r#"
                INSERT INTO source (bucket, key, version_id, task_status, error_code, http_status_code, result_message, checksum, checksum_algorithm)
                VALUES (?, ?, ?, ?, ?, ?, '', ?, ?)
                "#,
            )
            .unwrap();

        for r in source_rows {
            source_stmt
                .execute(duckdb::params![
                    r.bucket,
                    r.key,
                    r.version_id,
                    r.task_status,
                    r.error_code,
                    r.http_status_code,
                    r.checksum,
                    r.checksum_algorithm
                ])
                .unwrap();
        }

        let mut repl_stmt = conn
            .prepare(
                r#"
                INSERT INTO replication (bucket, key, version_id, task_status, error_code, http_status_code, result_message, checksum, checksum_algorithm)
                VALUES (?, ?, ?, ?, ?, ?, '', ?, ?)
                "#,
            )
            .unwrap();

        for r in replication_rows {
            repl_stmt
                .execute(duckdb::params![
                    r.bucket,
                    r.key,
                    r.version_id,
                    r.task_status,
                    r.error_code,
                    r.http_status_code,
                    r.checksum,
                    r.checksum_algorithm
                ])
                .unwrap();
        }

        ChecksumVerifier { conn }
    }

    // Tests using CSV fixtures (the fully happy path)
    #[test]
    fn test_load_valid_csv() {
        let verifier = ChecksumVerifier::load(
            &["../../files/checksum-source.csv"],
            &["../../files/checksum-replication.csv"],
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
            &["../../files/checksum-source.csv"],
            &["../../files/checksum-replication.csv"],
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
            &["../../files/checksum-source.csv"],
            &["../../files/checksum-replication.csv"],
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

    // Tests using in-memory data
    #[test]
    fn test_all_match() {
        let verifier = create_test_verifier(
            &[
                row("file1.pdf", "v1", "AAAA"),
                row("file2.pdf", "v1", "BBBB"),
                row("file3.pdf", "v1", "CCCC"),
            ],
            &[
                row("file1.pdf", "v1", "AAAA"),
                row("file2.pdf", "v1", "BBBB"),
                row("file3.pdf", "v1", "CCCC"),
            ],
        );

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.matches, 3);
        assert_eq!(stats.mismatches, 0);
        assert_eq!(stats.missing_replica, 0);
        assert_eq!(stats.missing_source, 0);
        assert_eq!(stats.failed_source, 0);
        assert_eq!(stats.failed_replication, 0);
        assert_eq!(stats.total_objects, 3);
    }

    #[test]
    fn test_mismatch() {
        let verifier = create_test_verifier(
            &[row("file.pdf", "v1", "AAAA")],
            &[row("file.pdf", "v1", "BBBB")],
        );

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.mismatches, 1);
        assert_eq!(stats.matches, 0);

        let mismatches = verifier.find_mismatches().unwrap();
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].key, "file.pdf");
        assert_eq!(mismatches[0].checksum_source, "AAAA");
        assert_eq!(mismatches[0].checksum_replication, "BBBB");
    }

    #[test]
    fn test_missing_from_replication() {
        let verifier = create_test_verifier(
            &[
                row("file1.pdf", "v1", "AAAA"),
                row("file2.pdf", "v1", "BBBB"),
            ],
            &[row("file1.pdf", "v1", "AAAA")],
        );

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.matches, 1);
        assert_eq!(stats.missing_replica, 1);

        let missing = verifier.objects_only_in_source().unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].key, "file2.pdf");
    }

    #[test]
    fn test_missing_from_source() {
        let verifier = create_test_verifier(
            &[row("file1.pdf", "v1", "AAAA")],
            &[
                row("file1.pdf", "v1", "AAAA"),
                row("file2.pdf", "v1", "BBBB"),
            ],
        );

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.matches, 1);
        assert_eq!(stats.missing_source, 1);

        let missing = verifier.objects_only_in_replication().unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].key, "file2.pdf");
    }

    #[test]
    fn test_failed_source() {
        let verifier = create_test_verifier(
            &[failed_row("file.pdf", "v1")],
            &[row("file.pdf", "v1", "AAAA")],
        );

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.failed_source, 1);
        assert_eq!(stats.matches, 0);

        let failed = verifier.failed_in_source().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].key, "file.pdf");
        assert_eq!(failed[0].error_code, "InternalError");
    }

    #[test]
    fn test_failed_replication() {
        let verifier = create_test_verifier(
            &[row("file.pdf", "v1", "AAAA")],
            &[failed_row("file.pdf", "v1")],
        );

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.failed_replication, 1);
        assert_eq!(stats.matches, 0);

        let failed = verifier.failed_in_replication().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].key, "file.pdf");
    }

    #[test]
    fn test_mixed_scenarios() {
        let verifier = create_test_verifier(
            &[
                row("match.pdf", "v1", "AAAA"),        // matches
                row("mismatch.pdf", "v1", "BBBB"),     // mismatch
                row("source-only.pdf", "v1", "CCCC"),  // missing from replication
                failed_row("failed-source.pdf", "v1"), // failed in source
            ],
            &[
                row("match.pdf", "v1", "AAAA"),      // matches
                row("mismatch.pdf", "v1", "XXXX"),   // mismatch (different checksum)
                row("repl-only.pdf", "v1", "DDDD"),  // missing from source
                failed_row("failed-repl.pdf", "v1"), // failed in replication
            ],
        );

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.matches, 1);
        assert_eq!(stats.mismatches, 1);
        assert_eq!(stats.missing_replica, 1);
        assert_eq!(stats.missing_source, 1);
        assert_eq!(stats.failed_source, 1);
        assert_eq!(stats.failed_replication, 1);
        assert_eq!(stats.total_objects, 6);
    }

    #[test]
    fn test_write_csv_mixed() {
        let verifier = create_test_verifier(
            &[
                row("match.pdf", "v1", "AAAA"),
                row("mismatch.pdf", "v1", "BBBB"),
            ],
            &[
                row("match.pdf", "v1", "AAAA"),
                row("mismatch.pdf", "v1", "XXXX"),
            ],
        );

        let mut output = Vec::new();
        verifier.write_csv(&mut output).unwrap();

        let csv = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = csv.lines().collect();

        assert_eq!(lines.len(), 3); // header + 2 rows
        assert!(csv.contains(",ok,"));
        assert!(csv.contains(",mismatch,"));
    }

    #[test]
    fn test_version_id_distinction() {
        // Same key but different versions should be treated as different objects
        let verifier = create_test_verifier(
            &[row("file.pdf", "v1", "AAAA"), row("file.pdf", "v2", "BBBB")],
            &[row("file.pdf", "v1", "AAAA"), row("file.pdf", "v2", "BBBB")],
        );

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.matches, 2);
        assert_eq!(stats.total_objects, 2);
    }

    #[test]
    fn test_empty_tables() {
        let verifier = create_test_verifier(&[], &[]);

        let stats = verifier.stats().unwrap();
        assert_eq!(stats.total_objects, 0);
        assert_eq!(stats.matches, 0);
    }
}
