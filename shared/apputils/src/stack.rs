pub const MANAGED_SUFFIX: &str = "-managed";
pub const REQUEST_SUFFIX: &str = "-bucket-request";

const BATCH_POLICY_SUFFIX: &str = "-s3-batch-policy";
const BATCH_ROLE_SUFFIX: &str = "-s3-batch-role";
const METADATA_PREFIX: &str = "metadata";
const REPLICATION_POLICY_SUFFIX: &str = "-s3-replication-policy";
const REPLICATION_ROLE_SUFFIX: &str = "-s3-replication-role";
const REPORTS_PREFIX: &str = "reports";

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

/// A type wrapper to ensure name conforms to minimal expectations.
#[derive(Debug, Clone)]
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

    /// Batch operations policy name for stack
    pub fn batch_policy_name(&self) -> String {
        format!("{}{}", &self.as_str(), BATCH_POLICY_SUFFIX)
    }

    /// Batch operations role name for stack
    pub fn batch_role_name(&self) -> String {
        format!("{}{}", &self.as_str(), BATCH_ROLE_SUFFIX)
    }

    /// Get managed bucket name for stack
    pub fn managed_bucket(&self) -> String {
        format!("{}{}", &self.as_str(), MANAGED_SUFFIX)
    }

    /// Replication policy name for stack
    pub fn replication_policy_name(&self) -> String {
        format!("{}{}", &self.as_str(), REPLICATION_POLICY_SUFFIX)
    }

    /// Replication role name for stack
    pub fn replication_role_name(&self) -> String {
        format!("{}{}", &self.as_str(), REPLICATION_ROLE_SUFFIX)
    }

    /// Checksums job receipt (json) destination used for checksum verification processing
    pub fn reports_checksums_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{}/{}/checksums/{}.json", METADATA_PREFIX, date_ctx, bucket)
    }

    /// File manifest (csv) upload destination provided for user access
    pub fn reports_manifest_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{}/{}/manifests/{}.csv", REPORTS_PREFIX, date_ctx, bucket)
    }

    /// Usage stats (json) destination used to generate storage reports
    pub fn reports_stats_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{}/{}/stats/{}.json", METADATA_PREFIX, date_ctx, bucket)
    }

    /// Storage report (html) destination provided for user access
    pub fn reports_storage_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!("{}/{}/storage/{}.html", REPORTS_PREFIX, date_ctx, bucket)
    }

    /// Request bucket name for stack
    pub fn request_bucket(&self) -> String {
        format!("{}{}", &self.as_str(), REQUEST_SUFFIX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_new() {
        assert_eq!(Name::new("test-stack").unwrap().as_str(), "test-stack");
        assert_eq!(Name::new("test-STaCK").unwrap().as_str(), "test-stack");
        assert!(Name::new(".test-stack").is_err());
        assert!(Name::new("test-stack.").is_err());
        assert!(Name::new("-test-stack").is_err());
        assert!(Name::new("test-stack-").is_err());
    }

    #[test]
    fn test_batch_policy_name() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(stack.batch_policy_name(), "test-stack-s3-batch-policy");
    }

    #[test]
    fn test_batch_role_name() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(stack.batch_role_name(), "test-stack-s3-batch-role");
    }

    #[test]
    fn test_managed_bucket_name() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(stack.managed_bucket(), "test-stack-managed");
    }

    #[test]
    fn test_replication_policy_name() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.replication_policy_name(),
            "test-stack-s3-replication-policy"
        );
    }

    #[test]
    fn test_replication_role_name() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.replication_role_name(),
            "test-stack-s3-replication-role"
        );
    }

    #[test]
    fn test_request_bucket_name() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(stack.request_bucket(), "test-stack-bucket-request");
    }

    #[test]
    fn test_reports_manifest_path() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_manifest_path("my-bucket", DateCtx::Latest),
            "reports/latest/manifests/my-bucket.csv"
        );
    }

    #[test]
    fn test_reports_stats_path() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_stats_path("my-bucket", DateCtx::Latest),
            "metadata/latest/stats/my-bucket.json"
        );
    }

    #[test]
    fn test_reports_storage_path() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_storage_path("my-bucket", DateCtx::Latest),
            "reports/latest/storage/my-bucket.html"
        );
    }
}
