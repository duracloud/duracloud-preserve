use std::fmt;
use std::io::Write;

use duckdb::Connection;

use crate::{errors::ProcessingError, stats::VerificationStats};

pub fn process(
    source_reports: &[impl AsRef<str>],
    replication_reports: &[impl AsRef<str>],
) -> Result<(Vec<u8>, VerificationStats), ProcessingError> {
    let verifier = ChecksumVerifier::load(source_reports, replication_reports)?;
    let mut csv = Vec::new();
    let stats = verifier.write_csv_and_stats(&mut csv)?;
    Ok((csv, stats))
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

/// Handles csv format checksum report files from S3
#[derive(Debug)]
pub struct ChecksumVerifier {
    conn: Connection,
}

impl ChecksumVerifier {
    pub fn load(
        source_reports: &[impl AsRef<str>],
        replication_reports: &[impl AsRef<str>],
    ) -> Result<Self, ProcessingError> {
        let conn = Connection::open_in_memory()?;

        conn.execute_batch(&format!(
            "{}\n{}",
            ChecksumVerifier::create_table_stmt("source", source_reports),
            ChecksumVerifier::create_table_stmt("replication", replication_reports)
        ))?;

        Ok(Self { conn })
    }

    /// Find all objects where checksums match
    pub fn find_matches(&self) -> Result<Vec<VerificationResult>, ProcessingError> {
        self.find_by_checksum("=", VerificationStatus::Ok)
    }

    /// Find all objects where checksums do not match
    pub fn find_mismatches(&self) -> Result<Vec<VerificationResult>, ProcessingError> {
        self.find_by_checksum("!=", VerificationStatus::Mismatch)
    }

    /// Find objects that exist in source but not in replication
    pub fn objects_only_in_source(&self) -> Result<Vec<VerificationResult>, ProcessingError> {
        self.only_in("source", "replication", VerificationStatus::MissingReplica)
    }

    /// Find objects that exist in replication but not in source
    pub fn objects_only_in_replication(&self) -> Result<Vec<VerificationResult>, ProcessingError> {
        self.only_in("replication", "source", VerificationStatus::MissingSource)
    }

    /// Find failed tasks in source
    pub fn failed_in_source(&self) -> Result<Vec<VerificationResult>, ProcessingError> {
        self.failed_in("source", VerificationStatus::FailedSource)
    }

    /// Find failed tasks in replication
    pub fn failed_in_replication(&self) -> Result<Vec<VerificationResult>, ProcessingError> {
        self.failed_in("replication", VerificationStatus::FailedReplication)
    }

    fn find_by_checksum(
        &self,
        op: &str,
        status: VerificationStatus,
    ) -> Result<Vec<VerificationResult>, ProcessingError> {
        let sql = format!(
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
                AND s.checksum {op} r.checksum
            "#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            results.push(VerificationResult {
                bucket: row.get(0)?,
                key: row.get(1)?,
                version_id: row.get(2)?,
                status: status.clone(),
                checksum_algorithm: row.get(3)?,
                checksum_source: row.get(4)?,
                checksum_replication: row.get(5)?,
            });
        }

        Ok(results)
    }

    fn only_in(
        &self,
        present: &str,
        missing: &str,
        status: VerificationStatus,
    ) -> Result<Vec<VerificationResult>, ProcessingError> {
        let sql = format!(
            r#"
            SELECT p.bucket, p.key, p.version_id
            FROM {present} p
            LEFT JOIN {missing} m
                ON p.key = m.key
                AND p.version_id = m.version_id
            WHERE m.key IS NULL
                AND p.task_status = 'succeeded'
            "#
        );
        self.collect_simple(&sql, status)
    }

    fn failed_in(
        &self,
        table: &str,
        status: VerificationStatus,
    ) -> Result<Vec<VerificationResult>, ProcessingError> {
        let sql = format!(
            r#"
            SELECT bucket, key, version_id
            FROM {table}
            WHERE task_status = 'failed'
            "#
        );
        self.collect_simple(&sql, status)
    }

    fn collect_simple(
        &self,
        sql: &str,
        status: VerificationStatus,
    ) -> Result<Vec<VerificationResult>, ProcessingError> {
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            results.push(VerificationResult {
                bucket: row.get(0)?,
                key: row.get(1)?,
                version_id: row.get(2)?,
                status: status.clone(),
                checksum_algorithm: String::new(),
                checksum_source: String::new(),
                checksum_replication: String::new(),
            });
        }

        Ok(results)
    }

    fn create_table_stmt(name: &str, from: &[impl AsRef<str>]) -> String {
        let from = from
            .iter()
            .map(|f| format!("'{}'", f.as_ref().replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            r#"
            CREATE TABLE {name} AS
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
            FROM read_csv([{from}]);
            "#
        )
    }

    fn write_csv_and_stats(
        &self,
        writer: impl Write,
    ) -> Result<VerificationStats, ProcessingError> {
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

        let matches = self.find_matches()?;
        let mismatches = self.find_mismatches()?;
        let only_in_source = self.objects_only_in_source()?;
        let only_in_replication = self.objects_only_in_replication()?;
        let failed_source = self.failed_in_source()?;
        let failed_replication = self.failed_in_replication()?;

        let stats = VerificationStats {
            matches: matches.len(),
            mismatches: mismatches.len(),
            missing_replica: only_in_source.len(),
            missing_source: only_in_replication.len(),
            failed_source: failed_source.len(),
            failed_replication: failed_replication.len(),
            total_objects: matches.len()
                + mismatches.len()
                + only_in_source.len()
                + only_in_replication.len()
                + failed_source.len()
                + failed_replication.len(),
        };

        for result in matches
            .into_iter()
            .chain(mismatches)
            .chain(only_in_source)
            .chain(only_in_replication)
            .chain(failed_source)
            .chain(failed_replication)
        {
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

        csv_writer.flush()?;
        Ok(stats)
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

    fn computed_stats(verifier: &ChecksumVerifier) -> VerificationStats {
        verifier.write_csv_and_stats(std::io::sink()).unwrap()
    }

    // Tests using CSV fixtures (the fully happy path)

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

        let stats = computed_stats(&verifier);
        assert_eq!(stats.matches, 3);
        assert_eq!(stats.mismatches, 0);
        assert_eq!(stats.missing_replica, 0);
        assert_eq!(stats.missing_source, 0);
        assert_eq!(stats.failed_source, 0);
        assert_eq!(stats.failed_replication, 0);
        assert_eq!(stats.total_objects, 3);
    }

    #[test]
    fn test_empty_tables() {
        let verifier = create_test_verifier(&[], &[]);

        let stats = computed_stats(&verifier);
        assert_eq!(stats.total_objects, 0);
        assert_eq!(stats.matches, 0);
    }

    #[test]
    fn test_failed_replication() {
        let verifier = create_test_verifier(
            &[row("file.pdf", "v1", "AAAA")],
            &[failed_row("file.pdf", "v1")],
        );

        let stats = computed_stats(&verifier);
        assert_eq!(stats.failed_replication, 1);
        assert_eq!(stats.matches, 0);

        let failed = verifier.failed_in_replication().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].key, "file.pdf");
    }

    #[test]
    fn test_failed_source() {
        let verifier = create_test_verifier(
            &[failed_row("file.pdf", "v1")],
            &[row("file.pdf", "v1", "AAAA")],
        );

        let stats = computed_stats(&verifier);
        assert_eq!(stats.failed_source, 1);
        assert_eq!(stats.matches, 0);

        let failed = verifier.failed_in_source().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].key, "file.pdf");
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
    fn test_mismatch() {
        let verifier = create_test_verifier(
            &[row("file.pdf", "v1", "AAAA")],
            &[row("file.pdf", "v1", "BBBB")],
        );

        let stats = computed_stats(&verifier);
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

        let stats = computed_stats(&verifier);
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

        let stats = computed_stats(&verifier);
        assert_eq!(stats.matches, 1);
        assert_eq!(stats.missing_source, 1);

        let missing = verifier.objects_only_in_replication().unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].key, "file2.pdf");
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

        let stats = computed_stats(&verifier);
        assert_eq!(stats.matches, 1);
        assert_eq!(stats.mismatches, 1);
        assert_eq!(stats.missing_replica, 1);
        assert_eq!(stats.missing_source, 1);
        assert_eq!(stats.failed_source, 1);
        assert_eq!(stats.failed_replication, 1);
        assert_eq!(stats.total_objects, 6);
    }

    #[test]
    fn test_process_returns_matching_csv_and_stats() {
        let (csv, stats) = process(
            &["../../files/checksum-source.csv"],
            &["../../files/checksum-replication.csv"],
        )
        .unwrap();

        assert_eq!(stats.total_objects, 4);
        assert_eq!(stats.matches, 4);
        assert_eq!(stats.mismatches, 0);
        assert_eq!(stats.missing_replica, 0);
        assert_eq!(stats.missing_source, 0);
        assert_eq!(stats.failed_source, 0);
        assert_eq!(stats.failed_replication, 0);

        let csv = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(csv.contains(",ok,"));
    }

    // Tests using in-memory data

    #[test]
    fn test_version_id_distinction() {
        // Same key but different versions should be treated as different objects
        let verifier = create_test_verifier(
            &[row("file.pdf", "v1", "AAAA"), row("file.pdf", "v2", "BBBB")],
            &[row("file.pdf", "v1", "AAAA"), row("file.pdf", "v2", "BBBB")],
        );

        let stats = computed_stats(&verifier);
        assert_eq!(stats.matches, 2);
        assert_eq!(stats.total_objects, 2);
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
        verifier.write_csv_and_stats(&mut output).unwrap();

        let csv = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = csv.lines().collect();

        assert_eq!(lines.len(), 3); // header + 2 rows
        assert!(csv.contains(",ok,"));
        assert!(csv.contains(",mismatch,"));
    }
}
