use constants::*;

use crate::{Stack, errors::BucketValidationError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket(Name, Type);

impl Bucket {
    pub fn new(name: &str, bucket_type: Type) -> Result<Self, BucketValidationError> {
        Ok(Self(Name::new(name)?, bucket_type))
    }

    pub fn bucket_type(&self) -> &Type {
        &self.1
    }

    pub fn name(&self) -> &str {
        self.0.as_str()
    }

    /// Convert a stack name + user requested bucket name to a full S3 primary bucket name.
    pub fn primary(stack: &Stack, partial: &Name) -> Result<Self, BucketValidationError> {
        if uses_reserved_prefix_or_suffix(stack.as_str(), partial.as_str()) {
            return Err(BucketValidationError::ValidationError(format!(
                "cannot use reserved prefix or suffix ({})",
                partial.as_str()
            )));
        }

        let name = format!(
            "{}{}{}",
            stack.as_str(),
            STACK_BUCKET_DELIMITER,
            partial.as_str()
        );
        if name.ends_with(PUBLIC_SUFFIX) {
            Bucket::new(&name, Type::Public)
        } else {
            Bucket::new(&name, Type::Standard)
        }
    }

    /// Convert a stack name + user requested bucket name to a full S3 replication bucket name.
    pub fn replication(stack: &Stack, partial: &Name) -> Result<Self, BucketValidationError> {
        let name = format!(
            "{}{}{}{}",
            stack.as_str(),
            STACK_BUCKET_DELIMITER,
            partial.as_str(),
            REPLICATION_SUFFIX
        );
        Bucket::new(&name, Type::Replication)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BucketPair {
    pub source: Bucket,
    pub replication: Bucket,
}

impl BucketPair {
    pub fn new(source: Bucket, replication: Bucket) -> Self {
        Self {
            source,
            replication,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name(String);

impl Name {
    pub fn new(name: &str) -> Result<Self, BucketValidationError> {
        let name = name.to_lowercase();

        if name.is_empty() {
            return Err(BucketValidationError::ValidationError(
                "name cannot be empty".to_string(),
            ));
        }

        for affix in DISALLOWED_AFFIXES {
            if name.starts_with(affix) || name.ends_with(affix) {
                return Err(BucketValidationError::ValidationError(format!(
                    "name cannot start or end with {} ({})",
                    affix, name
                )));
            }
        }

        if name.len() > MAX_LEN_FOR_NAME as usize {
            return Err(BucketValidationError::ValidationError(format!(
                "name cannot exceed total length of {} ({})",
                MAX_LEN_FOR_NAME, name
            )));
        }

        if Self::has_invalid_chars(&name) {
            return Err(BucketValidationError::ValidationError(format!(
                "name can only include alphanumberic or - characters ({})",
                name
            )));
        }

        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn has_invalid_chars(name: &str) -> bool {
        !name.chars().all(|c| c.is_alphanumeric() || c == '-')
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Internal,
    Public,
    Replication,
    Standard,
}

impl Type {
    pub fn from_tag_value(value: &str) -> Option<Self> {
        match value {
            "internal" => Some(Type::Internal),
            "public" => Some(Type::Public),
            "replication" => Some(Type::Replication),
            "standard" => Some(Type::Standard),
            _ => None,
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Internal => write!(f, "internal"),
            Type::Public => write!(f, "public"),
            Type::Replication => write!(f, "replication"),
            Type::Standard => write!(f, "standard"),
        }
    }
}

fn uses_reserved_prefix_or_suffix(prefix: &str, name: &str) -> bool {
    name.starts_with(prefix)
        || name.ends_with(MANAGED_SUFFIX)
        || name.ends_with(REPLICATION_SUFFIX)
        || name.ends_with(REQUEST_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_new() {
        assert!(Name::new("").is_err());

        // ok
        assert_eq!(Name::new("test").unwrap().as_str(), "test");
        assert_eq!(Name::new("TEsT").unwrap().as_str(), "test");
        assert_eq!(Name::new("test-stack").unwrap().as_str(), "test-stack");
        assert_eq!(Name::new("test-stack-1").unwrap().as_str(), "test-stack-1");

        // period as prefix or suffix
        assert!(Name::new(".test").is_err());
        assert!(Name::new("test.").is_err());

        // dash as prefix or suffix
        assert!(Name::new("-test").is_err());
        assert!(Name::new("test-").is_err());

        // length
        assert!(Name::new("t".repeat(MAX_LEN_FOR_NAME as usize).as_str()).is_ok());
        assert!(Name::new("t".repeat((MAX_LEN_FOR_NAME as usize) + 1).as_str()).is_err());

        // invalid chars
        assert!(Name::new("test ").is_err());
        assert!(Name::new("test_").is_err());
        assert!(Name::new("test@").is_err());
        assert!(Name::new("test.").is_err());
    }

    #[test]
    fn test_primary_public() {
        let stack = Stack::new("test-stack").unwrap();
        let public = Name::new("example-public").unwrap();

        let result = Bucket::primary(&stack, &public).unwrap();
        assert_eq!(result.name(), "test-stack-example-public");
        assert_eq!(result.bucket_type(), &Type::Public);
    }

    #[test]
    fn test_primary_reserved_validation() {
        let test_cases = vec!["test-stack", "test-request", "test-managed", "test-repl"];

        let stack = Stack::new("test-stack").unwrap();

        for name in test_cases {
            let bucket_name = Name::new(name).unwrap();
            let result = Bucket::primary(&stack, &bucket_name);

            assert!(result.is_err(), "Expected error for name: {}", name);
            match result.unwrap_err() {
                BucketValidationError::ValidationError(msg) => {
                    assert!(
                        msg.starts_with("cannot use reserved prefix or suffix"),
                        "Unexpected error message for name {}: {}",
                        name,
                        msg
                    );
                }
                _ => panic!("Expected ValidationError for name: {}", name),
            }
        }
    }

    #[test]
    fn test_primary_standard() {
        let stack = Stack::new("test-stack").unwrap();
        let standard = Name::new("example").unwrap();

        let result = Bucket::primary(&stack, &standard).unwrap();
        assert_eq!(result.name(), "test-stack-example");
        assert_eq!(result.bucket_type(), &Type::Standard);
    }

    #[test]
    fn test_replication_public() {
        let stack = Stack::new("test-stack").unwrap();
        let public = Name::new("example-public").unwrap();

        let result = Bucket::replication(&stack, &public).unwrap();
        assert_eq!(result.name(), "test-stack-example-public-repl");
        assert_eq!(result.bucket_type(), &Type::Replication);
    }

    #[test]
    fn test_replication_standard() {
        let stack = Stack::new("test-stack").unwrap();
        let standard = Name::new("example").unwrap();

        let result = Bucket::replication(&stack, &standard).unwrap();
        assert_eq!(result.name(), "test-stack-example-repl");
        assert_eq!(result.bucket_type(), &Type::Replication);
    }

    #[test]
    fn test_type_from_tag_value() {
        assert_eq!(Type::from_tag_value("internal"), Some(Type::Internal));
        assert_eq!(Type::from_tag_value("public"), Some(Type::Public));
        assert_eq!(Type::from_tag_value("replication"), Some(Type::Replication));
        assert_eq!(Type::from_tag_value("standard"), Some(Type::Standard));
        assert_eq!(Type::from_tag_value("unknown"), None);
        assert_eq!(Type::from_tag_value(""), None);
    }
}
