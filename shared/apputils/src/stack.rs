pub const MANAGED_SUFFIX: &str = "-managed";
pub const REQUEST_SUFFIX: &str = "-bucket-request";

const REPLICATION_ROLE_SUFFIX: &str = "-s3-replication-role";

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

    /// Get replication role name for stack
    pub fn replication_role_name(&self) -> String {
        format!("{}{}", &self.as_str(), REPLICATION_ROLE_SUFFIX)
    }

    /// Get request bucket name for stack
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
}
