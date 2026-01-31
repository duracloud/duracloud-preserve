pub const MANAGED_SUFFIX: &str = "-managed";
pub const REQUEST_SUFFIX: &str = "-bucket-request";

const BATCH_POLICY_SUFFIX: &str = "-s3-batch-policy";
const BATCH_ROLE_SUFFIX: &str = "-s3-batch-role";
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
        Ok(Self(Name::new(name)?))
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
        format!("{}{}", self.as_str(), BATCH_POLICY_SUFFIX)
    }

    /// Batch operations role name for stack
    pub fn batch_role_name(&self) -> String {
        format!("{}{}", self.as_str(), BATCH_ROLE_SUFFIX)
    }

    /// Get managed bucket name for stack
    pub fn managed_bucket(&self) -> String {
        format!("{}{}", self.as_str(), MANAGED_SUFFIX)
    }

    /// Checksums job receipt (json) destination used for checksum verification processing
    pub fn metadata_checksums_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{}/{}/checksums/{}.json", METADATA_PREFIX, date_ctx, bucket)
    }

    /// Usage stats (json) destination used to generate storage reports
    pub fn metadata_stats_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{}/{}/stats/{}.json", METADATA_PREFIX, date_ctx, bucket)
    }

    /// Replication policy name for stack
    pub fn replication_policy_name(&self) -> String {
        format!("{}{}", self.as_str(), REPLICATION_POLICY_SUFFIX)
    }

    /// Replication role name for stack
    pub fn replication_role_name(&self) -> String {
        format!("{}{}", self.as_str(), REPLICATION_ROLE_SUFFIX)
    }

    /// File manifest (csv) upload destination provided for user access
    pub fn reports_manifest_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{}/{}/manifests/{}.csv", REPORTS_PREFIX, date_ctx, bucket)
    }

    /// Storage report (html) destination provided for user access
    pub fn reports_storage_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{}/{}/storage/{}.html", REPORTS_PREFIX, date_ctx, bucket)
    }

    /// Request bucket name for stack
    pub fn request_bucket(&self) -> String {
        format!("{}{}", self.as_str(), REQUEST_SUFFIX)
    }
}

/// A type wrapper to ensure name conforms to minimal expectations.
#[derive(Debug, Clone, PartialEq)]
pub struct Name(String);
impl Name {
    // TODO: an actual error
    pub fn new(name: &str) -> Result<Self, &'static str> {
        let name = name.to_lowercase();

        for affix in DISALLOWED_AFFIXES {
            if name.starts_with(affix) || name.ends_with(affix) {
                return Err("Name cannot start or end with {affix}");
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
    fn test_stack_new() {
        assert_eq!(Stack::new("test-stack").unwrap().as_str(), "test-stack");
        assert_eq!(Stack::new("test-STaCK").unwrap().as_str(), "test-stack");
        assert!(Stack::new(".test-stack").is_err());
        assert!(Stack::new("test-stack.").is_err());
        assert!(Stack::new("-test-stack").is_err());
        assert!(Stack::new("test-stack-").is_err());
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
    fn test_batch_policy_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.batch_policy_name(), "test-stack-s3-batch-policy");
    }

    #[test]
    fn test_batch_role_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.batch_role_name(), "test-stack-s3-batch-role");
    }

    #[test]
    fn test_managed_bucket_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.managed_bucket(), "test-stack-managed");
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
    fn test_request_bucket_name() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(stack.request_bucket(), "test-stack-bucket-request");
    }

    #[test]
    fn test_reports_manifest_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_manifest_path("my-bucket", DateCtx::Latest),
            "reports/latest/manifests/my-bucket.csv"
        );
    }

    #[test]
    fn test_reports_stats_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.metadata_stats_path("my-bucket", DateCtx::Latest),
            "metadata/latest/stats/my-bucket.json"
        );
    }

    #[test]
    fn test_reports_storage_path() {
        let stack = Stack::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_storage_path("my-bucket", DateCtx::Latest),
            "reports/latest/storage/my-bucket.html"
        );
    }
}
