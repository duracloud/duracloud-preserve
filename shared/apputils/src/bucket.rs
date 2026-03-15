use std::collections::HashMap;

use crate::{
    Stack, content_type,
    errors::BucketValidationError,
    stack::{DISALLOWED_AFFIXES, STACK_BUCKET_DELIMITER},
};

pub const BUCKET_REQUEST_CONTENT_TYPE: &str = content_type::TEXT_PLAIN;
pub const BUCKET_TAG_STACK_KEY: &str = "Stack";
pub const BUCKET_TAG_TYPE_KEY: &str = "BucketType";

pub const MAX_BUCKETS_PER_REQUEST: u8 = 5;
pub const MAX_REQUEST_FILE_SIZE: u16 = 512;
pub const MAX_LEN_FOR_NAME: u8 = 63;

pub const PUBLIC_SUFFIX: &str = "-public";
pub const REPLICATION_SUFFIX: &str = "-repl";

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

/// Pair source buckets with their replication buckets.
/// Returns an error if any source bucket lacks a matching replication bucket.
pub fn pair_buckets(
    source_buckets: Vec<Bucket>,
    replication_buckets: Vec<Bucket>,
) -> Result<Vec<BucketPair>, BucketValidationError> {
    let mut repl_map: HashMap<String, Bucket> = replication_buckets
        .into_iter()
        .filter_map(|b| {
            let name = b.name().to_string();
            name.strip_suffix(REPLICATION_SUFFIX)
                .map(|base| (base.to_string(), b))
        })
        .collect();

    source_buckets
        .into_iter()
        .map(|source| {
            let source_name = source.name().to_string();
            repl_map
                .remove(&source_name)
                .map(|repl| BucketPair::new(source, repl))
                .ok_or_else(|| {
                    BucketValidationError::ValidationError(format!(
                        "no replication bucket found for '{}'",
                        source_name
                    ))
                })
        })
        .collect()
}

/// Convert a stack name + user requested bucket name to a full S3 primary bucket name.
pub fn primary_bucket(stack: &Stack, partial: &Name) -> Result<Bucket, BucketValidationError> {
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
pub fn replication_bucket(stack: &Stack, partial: &Name) -> Result<Bucket, BucketValidationError> {
    let name = format!(
        "{}{}{}{}",
        stack.as_str(),
        STACK_BUCKET_DELIMITER,
        partial.as_str(),
        REPLICATION_SUFFIX
    );
    Bucket::new(&name, Type::Replication)
}

/// Check that user supplied bucket names are valid and convert
/// to (primary, replication) pairs for the stack.
pub fn review_bucket_names(
    stack: &Stack,
    names: &[String],
) -> Result<Vec<BucketPair>, BucketValidationError> {
    let mut buckets: Vec<BucketPair> = Vec::new();

    for name in names {
        let bucket = Name::new(name)?;
        let primary = primary_bucket(stack, &bucket)?;
        let replication = replication_bucket(stack, &bucket)?;
        buckets.push(BucketPair::new(primary, replication));
    }

    Ok(buckets)
}

fn uses_reserved_prefix_or_suffix(prefix: &str, name: &str) -> bool {
    name.starts_with(prefix)
        || name.ends_with(REPLICATION_SUFFIX)
        || name.ends_with(crate::stack::MANAGED_SUFFIX)
        || name.ends_with(crate::stack::REQUEST_SUFFIX)
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
    fn test_pair_buckets() {
        let source_buckets = vec![
            Bucket::new("alpha", Type::Standard).unwrap(),
            Bucket::new("beta", Type::Public).unwrap(),
        ];
        let replication_buckets = vec![
            Bucket::new("beta-repl", Type::Replication).unwrap(),
            Bucket::new("alpha-repl", Type::Replication).unwrap(),
        ];

        let pairs = pair_buckets(source_buckets, replication_buckets).unwrap();

        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].source.name(), "alpha");
        assert_eq!(pairs[0].replication.name(), "alpha-repl");
        assert_eq!(pairs[1].source.name(), "beta");
        assert_eq!(pairs[1].replication.name(), "beta-repl");
    }

    #[test]
    fn test_pair_buckets_missing_replication() {
        let source_buckets = vec![
            Bucket::new("alpha", Type::Standard).unwrap(),
            Bucket::new("beta", Type::Public).unwrap(),
        ];
        let replication_buckets = vec![
            Bucket::new("alpha-repl", Type::Replication).unwrap(),
            // missing beta-repl
        ];

        let result = pair_buckets(source_buckets, replication_buckets);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, BucketValidationError::ValidationError(_)));
        assert!(err.to_string().contains("beta"));
    }

    #[test]
    fn test_request_primary_bucket_public() {
        let stack = Stack::new("test-stack").unwrap();
        let public = Name::new("example-public").unwrap();

        let result = primary_bucket(&stack, &public).unwrap();
        assert_eq!(result.name(), "test-stack-example-public");
        assert_eq!(result.bucket_type(), &Type::Public);
    }

    #[test]
    fn test_request_primary_bucket_reserved_validation() {
        let test_cases = vec![
            "test-stack",
            "test-bucket-request",
            "test-managed",
            "test-repl",
        ];

        let stack = Stack::new("test-stack").unwrap();

        for name in test_cases {
            let bucket_name = Name::new(name).unwrap();
            let result = primary_bucket(&stack, &bucket_name);

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
    fn test_request_primary_bucket_standard() {
        let stack = Stack::new("test-stack").unwrap();
        let standard = Name::new("example").unwrap();

        let result = primary_bucket(&stack, &standard).unwrap();
        assert_eq!(result.name(), "test-stack-example");
        assert_eq!(result.bucket_type(), &Type::Standard);
    }

    #[test]
    fn test_request_replication_bucket_public() {
        let stack = Stack::new("test-stack").unwrap();
        let public = Name::new("example-public").unwrap();

        let result = replication_bucket(&stack, &public).unwrap();
        assert_eq!(result.name(), "test-stack-example-public-repl");
        assert_eq!(result.bucket_type(), &Type::Replication);
    }

    #[test]
    fn test_request_replication_bucket_standard() {
        let stack = Stack::new("test-stack").unwrap();
        let standard = Name::new("example").unwrap();

        let result = replication_bucket(&stack, &standard).unwrap();
        assert_eq!(result.name(), "test-stack-example-repl");
        assert_eq!(result.bucket_type(), &Type::Replication);
    }

    #[test]
    fn test_review_bucket_names() {
        let stack = Stack::new("test-stack").unwrap();

        let names = vec!["example".to_string(), "data-public".to_string()];

        let result = review_bucket_names(&stack, &names).unwrap();

        assert_eq!(result.len(), 2);

        // First bucket pair (standard)
        assert_eq!(result[0].source.name(), "test-stack-example");
        assert_eq!(result[0].source.bucket_type(), &Type::Standard);
        assert_eq!(result[0].replication.name(), "test-stack-example-repl");
        assert_eq!(result[0].replication.bucket_type(), &Type::Replication);

        // Second bucket pair (public)
        assert_eq!(result[1].source.name(), "test-stack-data-public");
        assert_eq!(result[1].source.bucket_type(), &Type::Public);
        assert_eq!(result[1].replication.name(), "test-stack-data-public-repl");
        assert_eq!(result[1].replication.bucket_type(), &Type::Replication);
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
