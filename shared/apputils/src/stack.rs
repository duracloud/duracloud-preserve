pub const MANAGED_SUFFIX: &str = "-managed";
pub const REQUEST_SUFFIX: &str = "-bucket-request";

const BATCH_CHECKSUM_PREFIX: &str = "batch/reports/checksum";
const BATCH_POLICY_SUFFIX: &str = "-s3-batch-policy";
const BATCH_ROLE_SUFFIX: &str = "-s3-batch-role";
const FEEDBACK_PREFIX: &str = "feedback";
const METADATA_PREFIX: &str = "metadata";
const REPLICATION_POLICY_SUFFIX: &str = "-s3-replication-policy";
const REPLICATION_ROLE_SUFFIX: &str = "-s3-replication-role";
const REPORTS_PREFIX: &str = "reports";

/// Minimum number of `STACK_BUCKET_DELIMITER` parts in a valid bucket name.
pub const BUCKET_NAME_MIN_PARTS: usize = 3;
pub const DISALLOWED_AFFIXES: &[&str] = &[".", "-"];
pub const STACK_BUCKET_DELIMITER: &str = "-";

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

    /// Extract stack name from a bucket name.
    /// Stack is the first two delimited parts; bucket names must have at least `BUCKET_NAME_MIN_PARTS` parts.
    pub fn from_bucket_name(bucket: &str) -> Result<Self, &'static str> {
        let parts: Vec<&str> = bucket
            .splitn(BUCKET_NAME_MIN_PARTS, STACK_BUCKET_DELIMITER)
            .collect();

        if parts.len() < BUCKET_NAME_MIN_PARTS {
            return Err("Bucket name must have at least three parts");
        }

        let stack_name = format!("{}{}{}", parts[0], STACK_BUCKET_DELIMITER, parts[1]);
        Self::new(&stack_name)
    }

    /// Batch operations policy name for stack
    pub fn batch_policy_name(&self) -> String {
        format!("{}{BATCH_POLICY_SUFFIX}", self.as_str())
    }

    /// Batch compute checksums manifest upload path (when report is ready)
    pub fn batch_reports_checksum_manifest(&self, bucket: &str, job_id: &str) -> String {
        format!("{BATCH_CHECKSUM_PREFIX}/{bucket}/job-{job_id}/manifest.json")
    }

    /// Batch operations role name for stack
    pub fn batch_role_name(&self) -> String {
        format!("{}{BATCH_ROLE_SUFFIX}", self.as_str())
    }

    /// File upload path for feedback
    pub fn feedback_path(&self, file: &str) -> String {
        format!("{FEEDBACK_PREFIX}/{file}")
    }

    /// Get managed bucket name for stack
    pub fn managed_bucket(&self) -> String {
        format!("{}{MANAGED_SUFFIX}", self.as_str())
    }

    /// Checksums job receipt (json) destination used for checksum verification processing
    /// A valid identifier is either a source (not replication) bucket name or job id
    pub fn metadata_checksums_receipts_path(&self, identifier: &str, date_ctx: DateCtx) -> String {
        format!("{METADATA_PREFIX}/{date_ctx}/checksums/receipts/{identifier}.json")
    }

    /// Checksum verification stats destination
    pub fn metadata_checksums_stats_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{METADATA_PREFIX}/{date_ctx}/checksums/stats/{bucket}.json")
    }

    /// Inventory (per bucket) usage stats destination
    pub fn metadata_manifests_stats_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{METADATA_PREFIX}/{date_ctx}/manifests/stats/{bucket}.json")
    }

    /// Stack storage stats destination
    pub fn metadata_storage_stats_path(&self, date_ctx: DateCtx) -> String {
        format!(
            "{METADATA_PREFIX}/{date_ctx}/storage/stats/{}.json",
            self.as_str()
        )
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
    pub fn reports_checksums_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{REPORTS_PREFIX}/{date_ctx}/checksums/{bucket}.csv")
    }

    /// File manifest (csv) upload destination provided for user access
    pub fn reports_manifests_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{REPORTS_PREFIX}/{date_ctx}/manifests/{bucket}.csv")
    }

    /// Stack storage report (html) destination provided for user access
    pub fn reports_storage_path(&self, date_ctx: DateCtx) -> String {
        format!("{REPORTS_PREFIX}/{date_ctx}/storage/{}.html", self.as_str())
    }

    /// Request bucket name for stack
    pub fn request_bucket(&self) -> String {
        format!("{}{REQUEST_SUFFIX}", self.as_str())
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
    fn test_batch_policy_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.batch_policy_name(), "test-stack-s3-batch-policy");
    }

    #[test]
    fn test_batch_reports_checksum_manifest() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.batch_reports_checksum_manifest("my-bucket", "abc123"),
            "batch/reports/checksum/my-bucket/job-abc123/manifest.json"
        );
    }

    #[test]
    fn test_batch_role_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.batch_role_name(), "test-stack-s3-batch-role");
    }

    #[test]
    fn test_feedback_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.feedback_path("bucket-request/test.txt"),
            "feedback/bucket-request/test.txt"
        );
    }

    #[test]
    fn test_managed_bucket() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.managed_bucket(), "test-stack-managed");
    }

    #[test]
    fn test_metadata_checksums_receipts_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.metadata_checksums_receipts_path("my-bucket", DateCtx::Latest),
            "metadata/latest/checksums/receipts/my-bucket.json"
        );
    }

    #[test]
    fn test_metadata_checksums_stats_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.metadata_checksums_stats_path("my-bucket", DateCtx::Latest),
            "metadata/latest/checksums/stats/my-bucket.json"
        );
    }

    #[test]
    fn test_metadata_manifests_stats_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.metadata_manifests_stats_path("my-bucket", DateCtx::Latest),
            "metadata/latest/manifests/stats/my-bucket.json"
        );
    }

    #[test]
    fn test_metadata_storage_stats_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.metadata_storage_stats_path(DateCtx::Latest),
            "metadata/latest/storage/stats/test-stack.json"
        );
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
        assert_eq!(
            stack.reports_checksums_path("my-bucket", DateCtx::Latest),
            "reports/latest/checksums/my-bucket.csv"
        );
    }

    #[test]
    fn test_reports_manifests_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_manifests_path("my-bucket", DateCtx::Latest),
            "reports/latest/manifests/my-bucket.csv"
        );
    }

    #[test]
    fn test_reports_storage_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_storage_path(DateCtx::Latest),
            "reports/latest/storage/test-stack.html"
        );
    }

    #[test]
    fn test_request_bucket() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.request_bucket(), "test-stack-bucket-request");
    }

    #[test]
    fn test_stack_from_bucket_name() {
        // Valid bucket names: (input, expected_stack)
        let valid_cases = [
            ("test-stack-managed", "test-stack"),
            ("test-stack-bucket-request", "test-stack"),
            ("test-stack-example", "test-stack"),
            ("test-stack-something-something", "test-stack"),
            ("test-stack-something-public", "test-stack"),
            ("test-stack-something-repl", "test-stack"),
        ];

        for (input, expected) in valid_cases {
            assert_eq!(
                Stack::from_bucket_name(input).unwrap().as_str(),
                expected,
                "failed for input: {}",
                input
            );
        }

        // Invalid: not enough parts
        let invalid_cases = ["test-stack", "test", ""];

        for input in invalid_cases {
            assert!(
                Stack::from_bucket_name(input).is_err(),
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
