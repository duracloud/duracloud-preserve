use crate::{
    bucket_creator::BucketCreator,
    config::Config,
    file::{File, download},
};
use apputils::{
    Stack, content_type,
    stack::{DISALLOWED_AFFIXES, STACK_BUCKET_DELIMITER},
};

use aws_sdk_s3::Client;
use thiserror::Error;
use tokio::io::AsyncBufReadExt;

pub const BUCKET_REQUEST_CONTENT_TYPE: &str = content_type::TEXT_PLAIN;

// Tag keys used for bucket discovery
pub(crate) const BUCKET_TAG_STACK_KEY: &str = "Stack";
pub(crate) const BUCKET_TAG_TYPE_KEY: &str = "BucketType";

pub const MAX_BUCKETS_PER_REQUEST: u8 = 5;
pub const MAX_BUCKETS_REQUEST_FILE_SIZE: u16 = 512;
pub const MAX_LEN_FOR_REQUEST_NAME: u8 = 63;

const PUBLIC_SUFFIX: &str = "-public";
pub const REPLICATION_SUFFIX: &str = "-repl";

/// Create and setup an S3 bucket. If setup fails attempt to rollback.
/// Returns the BucketCreator for further operations (e.g., enable_replication).
async fn create<'a>(
    config: &'a Config,
    bucket: &'a Bucket,
) -> Result<BucketCreator<'a>, RequestError> {
    let creator = BucketCreator::new(config, bucket);

    creator.create().await?; // escape immediately if create fails

    if let Err(e) = creator.setup().await {
        if let Err(rollback_err) = creator.rollback().await {
            tracing::error!("Rollback failed: {}", rollback_err);
        }
        return Err(e);
    }

    Ok(creator)
}

/// Create primary and replication buckets
pub async fn create_buckets(config: &Config, buckets: &[(Bucket, Bucket)]) -> Vec<String> {
    let mut issues = Vec::new();

    for (primary, replication) in buckets {
        let result = create_bucket_pair(config, primary, replication).await;
        if let Err(e) = &result {
            issues.push(e.to_string());
        }
    }

    issues
}

/// Create a primary bucket and its replication bucket, then enable replication.
async fn create_bucket_pair(
    config: &Config,
    primary: &Bucket,
    replication: &Bucket,
) -> Result<(), RequestError> {
    let primary_creator = create(config, primary).await?;

    let repl_creator = match create(config, replication).await {
        Ok(creator) => creator,
        Err(e) => {
            if let Err(rollback_err) = primary_creator.rollback().await {
                tracing::error!("Rollback of {} failed: {}", primary.name(), rollback_err);
            }
            return Err(e);
        }
    };

    if let Err(e) = primary_creator.enable_replication(replication).await {
        if let Err(rollback_err) = primary_creator.rollback().await {
            tracing::error!("Rollback of {} failed: {}", primary.name(), rollback_err);
        }
        if let Err(rollback_err) = repl_creator.rollback().await {
            tracing::error!(
                "Rollback of {} failed: {}",
                replication.name(),
                rollback_err
            );
        }
        return Err(e);
    }

    Ok(())
}

/// Delete an empty bucket
pub async fn delete(client: &Client, bucket: &str) -> Result<(), RequestError> {
    client
        .delete_bucket()
        .bucket(bucket)
        .send()
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to delete bucket {}: {}", bucket, e)))?;

    Ok(())
}

/// Empty all objects from a bucket (handles versioned objects)
pub async fn empty(client: &Client, bucket: &str) -> Result<(), RequestError> {
    use aws_sdk_s3::types::{Delete, ObjectIdentifier};

    loop {
        let response = client
            .list_object_versions()
            .bucket(bucket)
            .max_keys(1000)
            .send()
            .await
            .map_err(|e| RequestError::S3Error(format!("failed to list versions: {}", e)))?;

        let mut objects_to_delete = Vec::new();

        for version in response.versions() {
            if let (Some(key), Some(version_id)) = (version.key(), version.version_id()) {
                objects_to_delete.push(
                    ObjectIdentifier::builder()
                        .key(key)
                        .version_id(version_id)
                        .build()
                        .map_err(|e| {
                            RequestError::S3Error(format!("failed to build object id: {}", e))
                        })?,
                );
            }
        }

        for marker in response.delete_markers() {
            if let (Some(key), Some(version_id)) = (marker.key(), marker.version_id()) {
                objects_to_delete.push(
                    ObjectIdentifier::builder()
                        .key(key)
                        .version_id(version_id)
                        .build()
                        .map_err(|e| {
                            RequestError::S3Error(format!("failed to build object id: {}", e))
                        })?,
                );
            }
        }

        if objects_to_delete.is_empty() {
            break;
        }

        client
            .delete_objects()
            .bucket(bucket)
            .delete(
                Delete::builder()
                    .set_objects(Some(objects_to_delete))
                    .build()
                    .map_err(|e| RequestError::S3Error(format!("failed to build delete: {}", e)))?,
            )
            .send()
            .await
            .map_err(|e| RequestError::S3Error(format!("failed to delete objects: {}", e)))?;
    }

    Ok(())
}

/// Check if a bucket exists
pub async fn exists(client: &Client, bucket: &str) -> bool {
    client.head_bucket().bucket(bucket).send().await.is_ok()
}

/// Retrieve bucket request file and check is valid
pub async fn get_bucket_names(client: &Client, file: &File) -> Result<Vec<String>, RequestError> {
    let Ok(r) = download(client, file).await else {
        return Err(RequestError::S3Error("failed to download file".to_string()));
    };

    if let Some(ct) = r.content_type()
        && !ct.starts_with(BUCKET_REQUEST_CONTENT_TYPE)
    {
        return Err(RequestError::InvalidContentType);
    }

    if let Some(len) = r.content_length()
        && len > MAX_BUCKETS_REQUEST_FILE_SIZE as i64
    {
        return Err(RequestError::FileTooLarge {
            actual: len,
            max: MAX_BUCKETS_REQUEST_FILE_SIZE as i64,
        });
    }

    let reader = r.body.into_async_read();
    let mut names = Vec::new();
    let mut buf_reader = tokio::io::BufReader::new(reader).lines();

    while let Ok(Some(line)) = buf_reader.next_line().await {
        names.push(line);
        if names.len() >= MAX_BUCKETS_PER_REQUEST as usize {
            break;
        }
    }

    Ok(names)
}

/// Get all buckets belonging to a stack (prefix match + stack tag)
pub async fn get_stack_buckets(
    client: &Client,
    stack: &Stack,
) -> Result<Vec<Bucket>, RequestError> {
    let prefix = format!("{}{}", stack.as_str(), STACK_BUCKET_DELIMITER);
    let mut buckets = Vec::new();

    let response = client
        .list_buckets()
        .send()
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to list buckets: {}", e)))?;

    for bucket in response.buckets() {
        let Some(name) = bucket.name() else {
            continue;
        };

        if !name.starts_with(&prefix) {
            continue;
        }

        // Check tags to verify it belongs to this stack (prefix alone is not sufficient)
        let Ok(tag_response) = client.get_bucket_tagging().bucket(name).send().await else {
            continue;
        };

        let tags = tag_response.tag_set();
        let stack_matches = tags
            .iter()
            .any(|tag| tag.key() == BUCKET_TAG_STACK_KEY && tag.value() == stack.as_str());

        if !stack_matches {
            continue;
        }

        if let Some(bucket) = Bucket::from_tags(name, tags)? {
            buckets.push(bucket);
        }
    }

    Ok(buckets)
}

/// Get stack buckets filtered by type
pub async fn get_stack_buckets_by_type(
    client: &Client,
    stack: &Stack,
    types: &[Type],
) -> Result<Vec<Bucket>, RequestError> {
    let all_buckets = get_stack_buckets(client, stack).await?;
    Ok(all_buckets
        .into_iter()
        .filter(|b| types.contains(b.bucket_type()))
        .collect())
}

/// Pair source buckets with their replication buckets.
/// Returns an error if any source bucket lacks a matching replication bucket.
pub fn pair_buckets(
    source_buckets: Vec<Bucket>,
    replication_buckets: Vec<Bucket>,
) -> Result<Vec<(Bucket, Bucket)>, RequestError> {
    let mut repl_map: std::collections::HashMap<String, Bucket> = replication_buckets
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
                .map(|repl| (source, repl))
                .ok_or_else(|| {
                    RequestError::S3Error(format!(
                        "no replication bucket found for '{}'",
                        source_name
                    ))
                })
        })
        .collect()
}

/// Check that user supplied bucket names are ok and make ready to create
pub fn review_bucket_names(
    config: &Config,
    names: &[String],
) -> Result<Vec<(Bucket, Bucket)>, RequestError> {
    let mut buckets: Vec<(Bucket, Bucket)> = Vec::new();

    for name in names {
        let bucket = Name::new(name)?;
        let primary = Request::primary_bucket(config.stack(), &bucket)?;
        let replication = Request::replication_bucket(config.stack(), &bucket)?;
        buckets.push((primary, replication));
    }

    Ok(buckets)
}

#[derive(Debug, PartialEq)]
pub struct Bucket(Name, Type);

impl Bucket {
    pub fn new(name: &str, bucket_type: Type) -> Result<Self, RequestError> {
        Ok(Self(Name::new(name)?, bucket_type))
    }

    pub fn bucket_type(&self) -> &Type {
        &self.1
    }

    pub fn name(&self) -> &str {
        self.0.as_str()
    }

    /// Fetch bucket from S3 and determine its type from tags.
    /// Returns `Ok(None)` if bucket doesn't exist or has no valid BucketType tag.
    pub async fn from_name(client: &Client, name: &str) -> Result<Option<Self>, RequestError> {
        let Ok(response) = client.get_bucket_tagging().bucket(name).send().await else {
            return Ok(None);
        };
        Self::from_tags(name, response.tag_set())
    }

    /// Construct from a tag set. Returns `None` if no valid BucketType tag is found.
    pub fn from_tags(
        name: &str,
        tags: &[aws_sdk_s3::types::Tag],
    ) -> Result<Option<Self>, RequestError> {
        let bucket_type = tags
            .iter()
            .find(|tag| tag.key() == BUCKET_TAG_TYPE_KEY)
            .and_then(|tag| Type::from_tag_value(tag.value()));

        match bucket_type {
            Some(t) => Ok(Some(Self::new(name, t)?)),
            None => Ok(None),
        }
    }
}

/// A type wrapper to ensure bucket name is compatible with
/// S3 and project requirements.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name(String);

impl Name {
    pub fn new(name: &str) -> Result<Self, RequestError> {
        let name = name.to_lowercase();

        if name.is_empty() {
            return Err(RequestError::ValidationError(
                "name cannot be empty".to_string(),
            ));
        }

        for affix in DISALLOWED_AFFIXES {
            if name.starts_with(affix) || name.ends_with(affix) {
                return Err(RequestError::ValidationError(format!(
                    "name cannot start or end with {} ({})",
                    affix, name
                )));
            }
        }

        if name.len() > MAX_LEN_FOR_REQUEST_NAME as usize {
            return Err(RequestError::ValidationError(format!(
                "name cannot exceed total length of {} ({})",
                MAX_LEN_FOR_REQUEST_NAME, name
            )));
        }

        if Self::has_invalid_chars(&name) {
            return Err(RequestError::ValidationError(format!(
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

/// Handles conversion of a stack name + user requested bucket
/// name to a full length S3 primary and replication bucket name.
#[derive(Debug)]
pub struct Request {}

impl Request {
    pub fn primary_bucket(stack: &Stack, partial: &Name) -> Result<Bucket, RequestError> {
        if Self::uses_reserved_prefix_or_suffix(stack.as_str(), partial.as_str()) {
            return Err(RequestError::ValidationError(format!(
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

    pub fn replication_bucket(stack: &Stack, partial: &Name) -> Result<Bucket, RequestError> {
        let name = format!(
            "{}{}{}{}",
            stack.as_str(),
            STACK_BUCKET_DELIMITER,
            partial.as_str(),
            REPLICATION_SUFFIX
        );
        Bucket::new(&name, Type::Replication)
    }

    // TODO: tidy this up
    fn uses_reserved_prefix_or_suffix(prefix: &str, name: &str) -> bool {
        name.starts_with(prefix)
            || name.ends_with(REPLICATION_SUFFIX)
            || name.ends_with(apputils::stack::MANAGED_SUFFIX)
            || name.ends_with(apputils::stack::REQUEST_SUFFIX)
    }
}

/// Custom error type for bucket requests
#[derive(Debug, Error)]
pub enum RequestError {
    #[error("File size {actual} bytes exceeds maximum of {max} bytes")]
    FileTooLarge { actual: i64, max: i64 },
    #[error("Content Type error: must be a text file")]
    InvalidContentType,
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("S3 error: {0}")]
    S3Error(String),
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// Types for buckets
#[derive(Debug, PartialEq)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_client::{TestClientBuilder, test_config_with_client_and_stack};

    #[test]
    fn test_type_from_tag_value() {
        assert_eq!(Type::from_tag_value("internal"), Some(Type::Internal));
        assert_eq!(Type::from_tag_value("public"), Some(Type::Public));
        assert_eq!(Type::from_tag_value("replication"), Some(Type::Replication));
        assert_eq!(Type::from_tag_value("standard"), Some(Type::Standard));
        assert_eq!(Type::from_tag_value("unknown"), None);
        assert_eq!(Type::from_tag_value(""), None);
    }

    #[test]
    fn test_bucket_from_tags() {
        use aws_sdk_s3::types::Tag;

        let tags_with_type = vec![
            Tag::builder().key("Other").value("value").build().unwrap(),
            Tag::builder()
                .key(BUCKET_TAG_TYPE_KEY)
                .value("standard")
                .build()
                .unwrap(),
        ];
        let result = Bucket::from_tags("test-bucket", &tags_with_type).unwrap();
        assert!(result.is_some());
        let bucket = result.unwrap();
        assert_eq!(bucket.name(), "test-bucket");
        assert_eq!(bucket.bucket_type(), &Type::Standard);

        let tags_without_type = vec![Tag::builder().key("Other").value("value").build().unwrap()];
        let result = Bucket::from_tags("test-bucket", &tags_without_type).unwrap();
        assert!(result.is_none());

        let empty_tags: Vec<Tag> = vec![];
        let result = Bucket::from_tags("test-bucket", &empty_tags).unwrap();
        assert!(result.is_none());
    }

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
        assert!(Name::new("t".repeat(MAX_LEN_FOR_REQUEST_NAME as usize).as_str()).is_ok());
        assert!(Name::new("t".repeat((MAX_LEN_FOR_REQUEST_NAME as usize) + 1).as_str()).is_err());

        // invalid chars
        assert!(Name::new("test ").is_err());
        assert!(Name::new("test_").is_err());
        assert!(Name::new("test@").is_err());
        assert!(Name::new("test.").is_err());
    }

    #[tokio::test]
    async fn test_get_bucket_names() {
        let content = "123\n456\n789\n234\n567\n890";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain".to_string()))
            .build();

        let names = get_bucket_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), 5);
        assert_eq!(names[0], "123");
        assert_eq!(names[1], "456");
        assert_eq!(names[2], "789");
        assert_eq!(names[3], "234");
    }

    #[tokio::test]
    async fn test_get_bucket_names_exceeds_size_limit() {
        let content = "a".repeat((MAX_BUCKETS_REQUEST_FILE_SIZE + 1) as usize);
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain".to_string()))
            .build();

        let result = get_bucket_names(&client, &file).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::FileTooLarge { actual, max }) => {
                assert_eq!(actual, (MAX_BUCKETS_REQUEST_FILE_SIZE + 1) as i64);
                assert_eq!(max, MAX_BUCKETS_REQUEST_FILE_SIZE as i64);
            }
            _ => panic!("Expected FileTooLarge error"),
        }
    }

    #[tokio::test]
    async fn test_get_bucket_names_content_type_with_charset() {
        let content = "bucket1\nbucket2";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain; charset=utf-8".to_string()))
            .build();

        let names = get_bucket_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "bucket1");
        assert_eq!(names[1], "bucket2");
    }

    #[tokio::test]
    async fn test_get_bucket_names_invalid_content_type() {
        let content = "bucket1\nbucket2";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("application/json".to_string()))
            .build();

        let result = get_bucket_names(&client, &file).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(RequestError::InvalidContentType)));
    }

    #[tokio::test]
    async fn test_get_bucket_names_truncates_at_max() {
        let content = "a\nb\nc\nd\ne\nf\ng";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain".to_string()))
            .build();

        let names = get_bucket_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), MAX_BUCKETS_PER_REQUEST as usize);
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
        assert_eq!(pairs[0].0.name(), "alpha");
        assert_eq!(pairs[0].1.name(), "alpha-repl");
        assert_eq!(pairs[1].0.name(), "beta");
        assert_eq!(pairs[1].1.name(), "beta-repl");
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
        assert!(matches!(err, RequestError::S3Error(_)));
        assert!(err.to_string().contains("beta"));
    }

    #[test]
    fn test_review_bucket_names() {
        let stack = Stack::new("test-stack").unwrap();
        let client = TestClientBuilder::new().ok().build();
        let config = test_config_with_client_and_stack(client, stack);

        let names = vec!["example".to_string(), "data-public".to_string()];

        let result = review_bucket_names(&config, &names).unwrap();

        assert_eq!(result.len(), 2);

        // First bucket pair (standard)
        assert_eq!(result[0].0.name(), "test-stack-example");
        assert_eq!(result[0].0.bucket_type(), &Type::Standard);
        assert_eq!(result[0].1.name(), "test-stack-example-repl");
        assert_eq!(result[0].1.bucket_type(), &Type::Replication);

        // Second bucket pair (public)
        assert_eq!(result[1].0.name(), "test-stack-data-public");
        assert_eq!(result[1].0.bucket_type(), &Type::Public);
        assert_eq!(result[1].1.name(), "test-stack-data-public-repl");
        assert_eq!(result[1].1.bucket_type(), &Type::Replication);
    }

    #[test]
    fn test_request_primary_bucket_standard() {
        let stack = Stack::new("test-stack").unwrap();
        let standard = Name::new("example").unwrap();

        let result = Request::primary_bucket(&stack, &standard).unwrap();
        assert_eq!(result.name(), "test-stack-example");
        assert_eq!(result.bucket_type(), &Type::Standard);
    }

    #[test]
    fn test_request_primary_bucket_public() {
        let stack = Stack::new("test-stack").unwrap();
        let public = Name::new("example-public").unwrap();

        let result = Request::primary_bucket(&stack, &public).unwrap();
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
            let result = Request::primary_bucket(&stack, &bucket_name);

            assert!(result.is_err(), "Expected error for name: {}", name);
            match result.unwrap_err() {
                RequestError::ValidationError(msg) => {
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
    fn test_request_replication_bucket_standard() {
        let stack = Stack::new("test-stack").unwrap();
        let standard = Name::new("example").unwrap();

        let result = Request::replication_bucket(&stack, &standard).unwrap();
        assert_eq!(result.name(), "test-stack-example-repl");
        assert_eq!(result.bucket_type(), &Type::Replication);
    }

    #[test]
    fn test_request_replication_bucket_public() {
        let stack = Stack::new("test-stack").unwrap();
        let public = Name::new("example-public").unwrap();

        let result = Request::replication_bucket(&stack, &public).unwrap();
        assert_eq!(result.name(), "test-stack-example-public-repl");
        assert_eq!(result.bucket_type(), &Type::Replication);
    }
}
