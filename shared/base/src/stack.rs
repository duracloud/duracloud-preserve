use constants::*;

/// Date context for stack related outputs (reports etc.)
#[derive(Debug, Clone, Copy)]
pub enum DateCtx {
    Latest,
    Today,
    Yesterday,
}

impl std::fmt::Display for DateCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use chrono::Utc;
        match self {
            DateCtx::Latest => write!(f, "latest"),
            DateCtx::Today => write!(f, "{}", Utc::now().format("%Y-%m-%d")),
            DateCtx::Yesterday => {
                write!(
                    f,
                    "{}",
                    (Utc::now() - chrono::Duration::days(1)).format("%Y-%m-%d")
                )
            }
        }
    }
}

/// An S3 file reference within the stack's managed bucket.
#[derive(Debug, Clone)]
pub struct ManagedFile {
    bucket: String,
    key: String,
}

impl ManagedFile {
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stack(Name);

impl Stack {
    pub fn new(name: &str) -> Result<Self, &'static str> {
        let name = Name::new(name)?;
        let parts: Vec<&str> = name.as_str().split(STACK_BUCKET_DELIMITER).collect();

        if parts.len() != 2 {
            return Err("Stack name must have exactly two parts separated by a hyphen");
        }

        for part in &parts {
            if part.len() < 2
                || !part.starts_with(|c: char| c.is_ascii_alphabetic())
                || !part.chars().all(|c| c.is_ascii_alphanumeric())
            {
                return Err(
                    "Stack name parts must start with a letter and contain only lowercase alphanumeric characters",
                );
            }
        }

        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Extract stack name from any identifier whose first two hyphen-separated parts
    /// are the stack name (e.g. bucket names, IAM group names).
    /// The input must have at least `BUCKET_NAME_MIN_PARTS` hyphen-separated parts.
    pub fn from_prefixed_name(name: &str) -> Result<Self, &'static str> {
        let parts: Vec<&str> = name
            .splitn(BUCKET_NAME_MIN_PARTS, STACK_BUCKET_DELIMITER)
            .collect();

        if parts.len() < BUCKET_NAME_MIN_PARTS {
            return Err("Name must have at least three hyphen-separated parts");
        }

        let stack_name = format!("{}{}{}", parts[0], STACK_BUCKET_DELIMITER, parts[1]);
        Self::new(&stack_name)
    }

    /// Batch manifest prefix for job processing
    pub fn batch_manifest_prefix(&self, job_type: &str) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{BATCH_MANIFEST_PREFIX}/{job_type}"),
        }
    }

    /// Batch operations policy name for stack
    pub fn batch_policy_name(&self) -> String {
        format!("{}{BATCH_POLICY_SUFFIX}", self.as_str())
    }

    /// Batch report prefix for job processing
    pub fn batch_report_prefix(&self, job_type: &str, source_bucket: &str) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{BATCH_REPORT_PREFIX}/{job_type}/{source_bucket}"),
        }
    }

    /// Batch compute checksums manifest upload path (when report is ready)
    pub fn batch_reports_checksum_manifest(&self, bucket: &str, job_id: &str) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{BATCH_CHECKSUM_PREFIX}/{bucket}/job-{job_id}/manifest.json"),
        }
    }

    /// Batch operations role name for stack
    pub fn batch_role_name(&self) -> String {
        format!("{}{BATCH_ROLE_SUFFIX}", self.as_str())
    }

    /// Bucket request file upload path
    pub fn bucket_request_path(&self, file: &str) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{BUCKET_REQUEST_PREFIX}/{file}"),
        }
    }

    /// File upload path for feedback
    pub fn feedback_path(&self, file: &str) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{FEEDBACK_PREFIX}/{file}"),
        }
    }

    /// Path to an inventory manifest (manifest.json)
    pub fn inventory_manifest_path(&self, bucket: &str, date_ctx: DateCtx) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{MANIFESTS_PREFIX}/{bucket}/inventory/{date_ctx}T01-00Z/manifest.json"),
        }
    }

    /// Logging prefix path for a bucket
    pub fn logging_prefix_path(&self, bucket: &str) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{LOGGING_PREFIX}/{bucket}/"),
        }
    }

    /// Get managed bucket name for stack
    pub fn managed_bucket(&self) -> String {
        format!("{}{MANAGED_SUFFIX}", self.as_str())
    }

    /// Checksums job receipt (json) destination used for checksum verification processing
    /// A valid identifier is either a source (not replication) bucket name or job id
    pub fn metadata_checksums_receipts_path(
        &self,
        identifier: &str,
        date_ctx: DateCtx,
    ) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{METADATA_PREFIX}/{date_ctx}/checksums/receipts/{identifier}.json"),
        }
    }

    /// Checksum verification stats destination
    pub fn metadata_checksums_stats_path(&self, bucket: &str, date_ctx: DateCtx) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{METADATA_PREFIX}/{date_ctx}/checksums/stats/{bucket}.json"),
        }
    }

    /// Inventory (per bucket) usage stats destination
    pub fn metadata_manifests_stats_path(&self, bucket: &str, date_ctx: DateCtx) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{METADATA_PREFIX}/{date_ctx}/manifests/stats/{bucket}.json"),
        }
    }

    /// Stack storage stats destination
    pub fn metadata_storage_stats_path(&self, date_ctx: DateCtx) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!(
                "{METADATA_PREFIX}/{date_ctx}/storage/stats/{}.json",
                self.as_str()
            ),
        }
    }

    /// Replication policy name for stack
    pub fn replication_policy_name(&self) -> String {
        format!("{}{REPLICATION_POLICY_SUFFIX}", self.as_str())
    }

    /// Replication role name for stack
    pub fn replication_role_name(&self) -> String {
        format!("{}{REPLICATION_ROLE_SUFFIX}", self.as_str())
    }

    /// Checksum verification report (csv) upload destination provided for user access
    pub fn reports_checksums_path(&self, bucket: &str, date_ctx: DateCtx) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{REPORTS_PREFIX}/{date_ctx}/checksums/{bucket}.csv"),
        }
    }

    /// File manifest (csv) upload destination provided for user access
    pub fn reports_manifests_path(&self, bucket: &str, date_ctx: DateCtx) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{REPORTS_PREFIX}/{date_ctx}/manifests/{bucket}.csv"),
        }
    }

    /// Stack storage report (html) destination provided for user access
    pub fn reports_storage_path(&self, date_ctx: DateCtx) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{REPORTS_PREFIX}/{date_ctx}/storage/{}.html", self.as_str()),
        }
    }

    /// Request bucket name for stack
    pub fn request_bucket(&self) -> String {
        format!("{}{REQUEST_SUFFIX}", self.as_str())
    }

    /// Storage capacity parameter name for stack
    pub fn storage_capacity_param_name(&self) -> String {
        format!("{}{STORAGE_CAPACITY_SUFFIX}", self.as_str())
    }

    /// Sync users file upload path
    pub fn sync_users_path(&self, file: &str) -> ManagedFile {
        ManagedFile {
            bucket: self.managed_bucket(),
            key: format!("{SYNC_USERS_PREFIX}/{file}"),
        }
    }
}

/// A type wrapper to ensure name conforms to minimal expectations.
#[derive(Debug, Clone, PartialEq)]
pub struct Name(String);
impl Name {
    pub fn new(name: &str) -> Result<Self, &'static str> {
        let name = name.to_lowercase();

        for affix in DISALLOWED_AFFIXES {
            if name.starts_with(affix) || name.ends_with(affix) {
                return Err(match *affix {
                    "." => "Name cannot start or end with '.'",
                    "-" => "Name cannot start or end with '-'",
                    _ => "Name cannot start or end with a disallowed character",
                });
            }
        }

        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_manifest_prefix() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.batch_manifest_prefix("checksum");
        assert_eq!(mf.key(), "batch/manifests/checksum");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_batch_policy_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.batch_policy_name(), "test-stack-s3-batch-policy");
    }

    #[test]
    fn test_batch_report_prefix() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.batch_report_prefix("checksum", "my-bucket");
        assert_eq!(mf.key(), "batch/reports/checksum/my-bucket");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_batch_reports_checksum_manifest() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.batch_reports_checksum_manifest("my-bucket", "abc123");
        assert_eq!(
            mf.key(),
            "batch/reports/checksum/my-bucket/job-abc123/manifest.json"
        );
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_batch_role_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.batch_role_name(), "test-stack-s3-batch-role");
    }

    #[test]
    fn test_bucket_request_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.bucket_request_path("request.txt");
        assert_eq!(mf.key(), "buckets/request.txt");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_feedback_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.feedback_path("bucket-request/test.txt");
        assert_eq!(mf.key(), "feedback/bucket-request/test.txt");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_logging_prefix_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.logging_prefix_path("my-bucket");
        assert_eq!(mf.key(), "audit/my-bucket/");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_managed_bucket() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.managed_bucket(), "test-stack-managed");
    }

    #[test]
    fn test_metadata_checksums_receipts_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.metadata_checksums_receipts_path("my-bucket", DateCtx::Latest);
        assert_eq!(
            mf.key(),
            "metadata/latest/checksums/receipts/my-bucket.json"
        );
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_metadata_checksums_stats_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.metadata_checksums_stats_path("my-bucket", DateCtx::Latest);
        assert_eq!(mf.key(), "metadata/latest/checksums/stats/my-bucket.json");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_metadata_manifests_stats_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.metadata_manifests_stats_path("my-bucket", DateCtx::Latest);
        assert_eq!(mf.key(), "metadata/latest/manifests/stats/my-bucket.json");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_metadata_storage_stats_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.metadata_storage_stats_path(DateCtx::Latest);
        assert_eq!(mf.key(), "metadata/latest/storage/stats/test-stack.json");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_replication_policy_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.replication_policy_name(),
            "test-stack-s3-replication-policy"
        );
    }

    #[test]
    fn test_replication_role_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.replication_role_name(),
            "test-stack-s3-replication-role"
        );
    }

    #[test]
    fn test_reports_checksums_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.reports_checksums_path("my-bucket", DateCtx::Latest);
        assert_eq!(mf.key(), "reports/latest/checksums/my-bucket.csv");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_reports_manifests_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.reports_manifests_path("my-bucket", DateCtx::Latest);
        assert_eq!(mf.key(), "reports/latest/manifests/my-bucket.csv");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_reports_storage_path() {
        let stack = Stack::new("test-stack").unwrap();
        let mf = stack.reports_storage_path(DateCtx::Latest);
        assert_eq!(mf.key(), "reports/latest/storage/test-stack.html");
        assert_eq!(mf.bucket(), "test-stack-managed");
    }

    #[test]
    fn test_request_bucket() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.request_bucket(), "test-stack-request");
    }

    #[test]
    fn test_stack_from_prefixed_name() {
        // Valid bucket names: (input, expected_stack)
        let valid_cases = [
            ("test-stack-managed", "test-stack"),
            ("test-stack-request", "test-stack"),
            ("test-stack-example", "test-stack"),
            ("test-stack-something-something", "test-stack"),
            ("test-stack-something-public", "test-stack"),
            ("test-stack-something-repl", "test-stack"),
        ];

        for (input, expected) in valid_cases {
            assert_eq!(
                Stack::from_prefixed_name(input).unwrap().as_str(),
                expected,
                "failed for input: {}",
                input
            );
        }

        // Invalid: not enough parts
        let invalid_cases = ["test-stack", "test", ""];

        for input in invalid_cases {
            assert!(
                Stack::from_prefixed_name(input).is_err(),
                "expected error for input: {}",
                input
            );
        }
    }

    #[test]
    fn test_stack_new() {
        // Valid: two lowercase alphanumeric parts, each starting with a letter
        assert_eq!(Stack::new("test-stack").unwrap().as_str(), "test-stack");
        assert_eq!(Stack::new("test-STaCK").unwrap().as_str(), "test-stack");
        assert_eq!(
            Stack::new("digipress-dev1").unwrap().as_str(),
            "digipress-dev1"
        );
        assert_eq!(Stack::new("my-stack2").unwrap().as_str(), "my-stack2");

        // Invalid: affix violations (verify message includes actual character)
        assert_eq!(
            Stack::new(".test-stack").unwrap_err(),
            "Name cannot start or end with '.'"
        );
        assert_eq!(
            Stack::new("test-stack.").unwrap_err(),
            "Name cannot start or end with '.'"
        );
        assert_eq!(
            Stack::new("-test-stack").unwrap_err(),
            "Name cannot start or end with '-'"
        );
        assert_eq!(
            Stack::new("test-stack-").unwrap_err(),
            "Name cannot start or end with '-'"
        );

        // Invalid: not exactly two parts
        assert!(Stack::new("test").is_err());
        assert!(Stack::new("test-stack-extra").is_err());
        assert!(Stack::new("a-b-c").is_err());

        // Invalid: parts too short (min 2 chars each)
        assert!(Stack::new("a-bb").is_err());
        assert!(Stack::new("aa-b").is_err());

        // Invalid: part starts with digit
        assert!(Stack::new("1test-stack").is_err());
        assert!(Stack::new("test-1stack").is_err());

        // Invalid: non-alphanumeric characters
        assert!(Stack::new("test_-stack").is_err());
        assert!(Stack::new("test-sta.ck").is_err());
    }
}
