pub const MANAGED_SUFFIX: &str = "-managed";
pub const REQUEST_SUFFIX: &str = "-bucket-request";

const METADATA_PREFIX: &str = "metadata";
const REPLICATION_ROLE_SUFFIX: &str = "-s3-replication-role";
const REPORTS_PREFIX: &str = "reports";

/// Date context for stack related outputs (reports etc.)
pub enum DateCtx {
    Latest,
    Today,
}

impl std::fmt::Display for DateCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DateCtx::Latest => write!(f, "latest"),
            DateCtx::Today => todo!(),
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

        if name.starts_with("-") || name.ends_with("-") {
            return Err("Name cannot start or end with dash");
        }

        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get managed bucket name for stack
    pub fn managed_bucket(&self) -> String {
        format!("{}{}", &self.as_str(), MANAGED_SUFFIX)
    }

    /// Manifest json destination used for batch operations
    pub fn metadata_manifest_path(&self, bucket: &str) -> String {
        format!(
            "{}/{}/{}/manifests/{}.json",
            &self.as_str(),
            METADATA_PREFIX,
            DateCtx::Latest,
            bucket
        )
    }

    /// Stats json destination used for storage reports
    pub fn metadata_stats_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!(
            "{}/{}/{}/stats/{}.json",
            &self.as_str(),
            METADATA_PREFIX,
            date_ctx,
            bucket
        )
    }

    /// Replication role name for stack
    pub fn replication_role_name(&self) -> String {
        format!("{}{}", &self.as_str(), REPLICATION_ROLE_SUFFIX)
    }

    /// Manifest csv destination for user access
    pub fn reports_manifest_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!(
            "{}/{}/{}/manifests/{}.csv",
            &self.as_str(),
            REPORTS_PREFIX,
            date_ctx,
            bucket
        )
    }

    /// Storage html reports destination for user access
    pub fn reports_storage_path(&self, bucket: &str, date_ctx: DateCtx) -> String {
        format!(
            "{}/{}/{}/storage/{}.html",
            &self.as_str(),
            REPORTS_PREFIX,
            date_ctx,
            bucket
        )
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
        assert!(Name::new("-test-stack").is_err());
        assert!(Name::new("test-stack-").is_err());
    }

    #[test]
    fn test_managed_bucket_name() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(stack.managed_bucket(), "test-stack-managed");
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
    fn test_metadata_manifest_path() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.metadata_manifest_path("my-bucket"),
            "test-stack/metadata/latest/manifests/my-bucket.json"
        );
    }

    #[test]
    fn test_metadata_stats_path() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.metadata_stats_path("my-bucket", DateCtx::Latest),
            "test-stack/metadata/latest/stats/my-bucket.json"
        );
    }

    #[test]
    fn test_reports_manifest_path() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_manifest_path("my-bucket", DateCtx::Latest),
            "test-stack/reports/latest/manifests/my-bucket.csv"
        );
    }

    #[test]
    fn test_reports_storage_path() {
        let stack = Name::new("test-stack").unwrap();
        assert_eq!(
            stack.reports_storage_path("my-bucket", DateCtx::Latest),
            "test-stack/reports/latest/storage/my-bucket.html"
        );
    }
}
