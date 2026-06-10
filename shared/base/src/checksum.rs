use std::path::Path;

use duckdb::Connection;

use crate::{errors::ProcessingError, safe_join, stats::VerificationStats};

/// View unioning every verification category into a single relation: one row
/// per object, a literal `status` column, and a `sort` ordinal preserving the
/// report's category order.
const CREATE_RESULTS_VIEW: &str = r#"
    CREATE VIEW results AS

    -- ok / mismatch: object verified on both sides, so compare checksums.
    -- Rows whose checksum could not be extracted from the batch result
    -- message belong to neither category.
    SELECT CASE WHEN s.checksum = r.checksum THEN 0 ELSE 1 END AS sort,
           s.bucket, s.key, s.version_id,
           CASE WHEN s.checksum = r.checksum THEN 'ok' ELSE 'mismatch' END AS status,
           s.checksum_algorithm, s.checksum AS checksum_source, r.checksum AS checksum_replication
    FROM source s
    JOIN replication r ON s.key = r.key AND s.version_id = r.version_id
    WHERE s.task_status = 'succeeded' AND r.task_status = 'succeeded'
        AND s.checksum IS NOT NULL AND r.checksum IS NOT NULL

    UNION ALL

    -- missing_replica: verified in source but no replication row exists for
    -- the key/version (not even a failed one). Checksum columns are NULL so
    -- COPY renders them as unquoted empty CSV fields.
    SELECT 2, p.bucket, p.key, p.version_id, 'missing_replica', NULL, NULL, NULL
    FROM source p
    LEFT JOIN replication m ON p.key = m.key AND p.version_id = m.version_id
    WHERE m.key IS NULL AND p.task_status = 'succeeded'

    UNION ALL

    -- missing_source: verified in replication but no source row exists for
    -- the key/version (not even a failed one).
    SELECT 3, p.bucket, p.key, p.version_id, 'missing_source', NULL, NULL, NULL
    FROM replication p
    LEFT JOIN source m ON p.key = m.key AND p.version_id = m.version_id
    WHERE m.key IS NULL AND p.task_status = 'succeeded'

    UNION ALL

    -- failed_source: the source-side checksum batch task itself failed.
    SELECT 4, bucket, key, version_id, 'failed_source', NULL, NULL, NULL
    FROM source WHERE task_status = 'failed'

    UNION ALL

    -- failed_replication: the replication-side checksum batch task itself failed.
    SELECT 5, bucket, key, version_id, 'failed_replication', NULL, NULL, NULL
    FROM replication WHERE task_status = 'failed';
"#;

pub fn process(
    source_reports: &[impl AsRef<str>],
    replication_reports: &[impl AsRef<str>],
    output_csv: &Path,
) -> Result<VerificationStats, ProcessingError> {
    let verifier = ChecksumVerifier::load(source_reports, replication_reports)?;
    verifier.write_csv(output_csv)?;
    verifier.compute_stats()
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
            "{}\n{}\n{}",
            ChecksumVerifier::create_view_stmt("source", source_reports),
            ChecksumVerifier::create_view_stmt("replication", replication_reports),
            CREATE_RESULTS_VIEW
        ))?;

        Ok(Self { conn })
    }

    fn create_view_stmt(name: &str, from: &[impl AsRef<str>]) -> String {
        let files = safe_join(from);

        format!(
            r#"
            CREATE VIEW {name} AS
            SELECT
                column0 AS bucket,
                column1 AS key,
                column2 AS version_id,
                column3 AS task_status,
                column4 AS error_code,
                column5 AS http_status_code,
                column6 AS result_message,
                json_extract_string(column6, '$.checksum_base64') AS checksum,
                json_extract_string(column6, '$.checksumAlgorithm') AS checksum_algorithm
            FROM read_csv([{files}]);
            "#
        )
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
                    version_id,
                    status,
                    checksum_algorithm,
                    checksum_source,
                    checksum_replication
                FROM results
                ORDER BY sort
            ) TO {} (FORMAT CSV, HEADER)
            "#,
            safe_join(&[output.to_string_lossy()])
        ))?;

        Ok(())
    }

    /// Tally per-category counts with a single aggregate query
    fn compute_stats(&self) -> Result<VerificationStats, ProcessingError> {
        let mut stmt = self
            .conn
            .prepare("SELECT status, count(*)::UBIGINT FROM results GROUP BY status")?;
        let mut rows = stmt.query([])?;

        let mut stats = VerificationStats::default();
        while let Some(row) = rows.next()? {
            let status: String = row.get(0)?;
            let count = row.get::<_, u64>(1)? as usize;
            match status.as_str() {
                "ok" => stats.matches = count,
                "mismatch" => stats.mismatches = count,
                "missing_replica" => stats.missing_replica = count,
                "missing_source" => stats.missing_source = count,
                "failed_source" => stats.failed_source = count,
                "failed_replication" => stats.failed_replication = count,
                other => unreachable!("unexpected status in results view: {other}"),
            }
        }

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

        conn.execute_batch(CREATE_RESULTS_VIEW).unwrap();

        ChecksumVerifier { conn }
    }

    fn computed_stats(verifier: &ChecksumVerifier) -> VerificationStats {
        verifier.compute_stats().unwrap()
    }

    /// Run `write_csv` against a temp output file, returning the CSV text.
    fn write_csv_to_temp(verifier: &ChecksumVerifier) -> String {
        let temp_dir = tempfile::tempdir().unwrap();
        let output = temp_dir.path().join("checksum-report.csv");
        verifier.write_csv(&output).unwrap();
        std::fs::read_to_string(&output).unwrap()
    }

    // Tests using CSV fixtures (the fully happy path)

    struct Fixture {
        source: &'static str,
        replication: &'static str,
        sample_key: &'static str,
        sample_algorithm: &'static str,
        sample_checksum: &'static str,
        row_count: i64,
    }

    fn fixtures() -> [Fixture; 2] {
        [
            Fixture {
                source: "../../files/checksum-source_sha256.csv",
                replication: "../../files/checksum-replication_sha256.csv",
                sample_key: "wasteland01elio.pdf",
                sample_algorithm: "SHA256",
                sample_checksum: "l2kHKxgAOCnRFlcGDcmNz7XJbVDBl79A60DZLi0teGk=",
                row_count: 4,
            },
            Fixture {
                source: "../../files/checksum-source_crc64nvme.csv",
                replication: "../../files/checksum-replication_crc64nvme.csv",
                sample_key: "ARCHIVEIT-2135-ANNUAL-JOB2682296-SEED1387333-20260329172559707-00000-h3.warc.gz",
                sample_algorithm: "CRC64NVME",
                sample_checksum: "Jc59MrAghVY=",
                row_count: 2,
            },
        ]
    }

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
        assert_eq!(stats.total_objects(), 3);
    }

    #[test]
    fn test_empty_tables() {
        let verifier = create_test_verifier(&[], &[]);

        let stats = computed_stats(&verifier);
        assert_eq!(stats.total_objects(), 0);
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

        let csv = write_csv_to_temp(&verifier);
        assert!(
            csv.lines()
                .any(|l| l == "test-bucket,file.pdf,v1,failed_replication,,,"),
            "row not found in: {csv}"
        );
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

        let csv = write_csv_to_temp(&verifier);
        assert!(
            csv.lines()
                .any(|l| l == "test-bucket,file.pdf,v1,failed_source,,,"),
            "row not found in: {csv}"
        );
    }

    #[test]
    fn test_load_extracts_algorithm() {
        for fx in fixtures() {
            let verifier = ChecksumVerifier::load(&[fx.source], &[fx.replication]).unwrap();

            let algorithm: String = verifier
                .conn
                .query_row(
                    "SELECT checksum_algorithm FROM source WHERE key = ?",
                    [fx.sample_key],
                    |row| row.get(0),
                )
                .unwrap();

            assert_eq!(algorithm, fx.sample_algorithm, "fixture: {}", fx.source);
        }
    }

    #[test]
    fn test_load_extracts_checksum() {
        for fx in fixtures() {
            let verifier = ChecksumVerifier::load(&[fx.source], &[fx.replication]).unwrap();

            let checksum: String = verifier
                .conn
                .query_row(
                    "SELECT checksum FROM source WHERE key = ?",
                    [fx.sample_key],
                    |row| row.get(0),
                )
                .unwrap();

            assert_eq!(checksum, fx.sample_checksum, "fixture: {}", fx.source);
        }
    }

    #[test]
    fn test_load_valid_csv() {
        for fx in fixtures() {
            let verifier = ChecksumVerifier::load(&[fx.source], &[fx.replication]).unwrap();

            let source_count: i64 = verifier
                .conn
                .query_row("SELECT COUNT(*) FROM source", [], |row| row.get(0))
                .unwrap();
            let replication_count: i64 = verifier
                .conn
                .query_row("SELECT COUNT(*) FROM replication", [], |row| row.get(0))
                .unwrap();

            assert_eq!(source_count, fx.row_count, "fixture: {}", fx.source);
            assert_eq!(
                replication_count, fx.row_count,
                "fixture: {}",
                fx.replication
            );
        }
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

        let csv = write_csv_to_temp(&verifier);
        assert!(
            csv.lines()
                .any(|l| l == "test-bucket,file.pdf,v1,mismatch,SHA256,AAAA,BBBB"),
            "row not found in: {csv}"
        );
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

        let csv = write_csv_to_temp(&verifier);
        assert!(
            csv.lines()
                .any(|l| l == "test-bucket,file2.pdf,v1,missing_replica,,,"),
            "row not found in: {csv}"
        );
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

        let csv = write_csv_to_temp(&verifier);
        assert!(
            csv.lines()
                .any(|l| l == "test-bucket,file2.pdf,v1,missing_source,,,"),
            "row not found in: {csv}"
        );
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
        assert_eq!(stats.total_objects(), 6);
    }

    #[test]
    fn test_process_returns_matching_csv_and_stats() {
        for fx in fixtures() {
            let temp_dir = tempfile::tempdir().unwrap();
            let output = temp_dir.path().join("checksum-report.csv");
            let stats = process(&[fx.source], &[fx.replication], &output).unwrap();
            let expected = fx.row_count as usize;

            assert_eq!(stats.total_objects(), expected, "fixture: {}", fx.source);
            assert_eq!(stats.matches, expected, "fixture: {}", fx.source);
            assert_eq!(stats.mismatches, 0, "fixture: {}", fx.source);
            assert_eq!(stats.missing_replica, 0, "fixture: {}", fx.source);
            assert_eq!(stats.missing_source, 0, "fixture: {}", fx.source);
            assert_eq!(stats.failed_source, 0, "fixture: {}", fx.source);
            assert_eq!(stats.failed_replication, 0, "fixture: {}", fx.source);

            let csv = std::fs::read_to_string(&output).unwrap();
            let lines: Vec<&str> = csv.lines().collect();
            assert_eq!(lines.len(), expected + 1, "fixture: {}", fx.source);
            assert_eq!(
                lines[0],
                "bucket,key,version_id,status,checksum_algorithm,checksum_source,checksum_replication",
                "fixture: {}",
                fx.source
            );
            assert!(csv.contains(",ok,"), "fixture: {}", fx.source);
        }
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
        assert_eq!(stats.total_objects(), 2);
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

        let csv = write_csv_to_temp(&verifier);
        let lines: Vec<&str> = csv.lines().collect();

        assert_eq!(lines.len(), 3); // header + 2 rows
        assert!(csv.contains(",ok,"));
        assert!(csv.contains(",mismatch,"));
    }
}
