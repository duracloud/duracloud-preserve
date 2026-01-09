use crate::{
    bucket_creator::BucketCreator,
    file::{File, download},
};
use apputils::StackName;

use aws_sdk_s3::Client;
use thiserror::Error;
use tokio::io::AsyncBufReadExt;

pub const BUCKET_REQUEST_CONTENT_TYPE: &str = "text/plain";

// Tag keys used for bucket discovery
pub(crate) const BUCKET_TAG_STACK_KEY: &str = "Stack";
pub(crate) const BUCKET_TAG_TYPE_KEY: &str = "BucketType";

pub const MAX_BUCKETS_PER_REQUEST: u8 = 5;
pub const MAX_BUCKETS_REQUEST_FILE_SIZE: u8 = 32;
pub const MAX_LEN_FOR_REQUEST_NAME: u8 = 63;

const PUBLIC_SUFFIX: &str = "-public";
const REPLICATION_SUFFIX: &str = "-repl";

/// Check if a bucket exists
pub async fn bucket_exists(client: &Client, bucket: &str) -> bool {
    client.head_bucket().bucket(bucket).send().await.is_ok()
}

/// Create primary and replication buckets
pub async fn create_buckets(
    config: &RequestConfig,
    buckets: &Vec<(Bucket, Bucket)>,
) -> Vec<String> {
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
    config: &RequestConfig,
    primary: &Bucket,
    replication: &Bucket,
) -> Result<(), RequestError> {
    create_bucket(config, primary).await?;
    create_bucket(config, replication).await?;

    let creator = BucketCreator::new(config, primary);
    creator.enable_replication(replication).await?;

    Ok(())
}

/// Create and setup an S3 bucket. If setup fails attempt to rollback.
async fn create_bucket(config: &RequestConfig, bucket: &Bucket) -> Result<(), RequestError> {
    let creator = BucketCreator::new(config, bucket);

    creator.create().await?; // escape immediately if create fails

    let result = creator.setup().await;
    if let Err(e) = result {
        if let Err(rollback_err) = creator.rollback().await {
            eprintln!("Rollback failed: {}", rollback_err);
        }
        return Err(e);
    }

    Ok(())
}

/// Delete an empty bucket
pub async fn delete_bucket(client: &Client, bucket: &str) -> Result<(), RequestError> {
    client
        .delete_bucket()
        .bucket(bucket)
        .send()
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to delete bucket {}: {}", bucket, e)))?;

    Ok(())
}

/// Empty all objects from a bucket (handles versioned objects)
pub async fn empty_bucket(client: &Client, bucket: &str) -> Result<(), RequestError> {
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

/// Retrieve bucket request file and check is valid
pub async fn get_bucket_names(client: &Client, file: &File) -> Result<Vec<String>, RequestError> {
    let Ok(r) = download(&client, &file).await else {
        return Err(RequestError::S3Error("failed to download file".to_string()));
    };

    if let Some(ct) = r.content_type()
        && ct != BUCKET_REQUEST_CONTENT_TYPE
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

/// Check bucket tags and return the type if it belongs to the stack
async fn get_bucket_stack_type(client: &Client, bucket: &str, stack: &StackName) -> Option<Type> {
    let response = client
        .get_bucket_tagging()
        .bucket(bucket)
        .send()
        .await
        .ok()?;

    let tags = response.tag_set();
    let mut stack_matches = false;
    let mut bucket_type = None;

    for tag in tags {
        if tag.key() == BUCKET_TAG_STACK_KEY && tag.value() == stack.as_str() {
            stack_matches = true;
        }
        if tag.key() == BUCKET_TAG_TYPE_KEY {
            bucket_type = match tag.value() {
                "standard" => Some(Type::Standard),
                "public" => Some(Type::Public),
                "replication" => Some(Type::Replication),
                "managed" => Some(Type::Managed),
                "request" => Some(Type::Request),
                _ => None,
            };
        }
    }

    if stack_matches { bucket_type } else { None }
}

/// Get all buckets belonging to a stack (prefix match + stack tag)
pub async fn get_stack_buckets(
    client: &Client,
    stack: &StackName,
) -> Result<Vec<Bucket>, RequestError> {
    let prefix = format!("{}-", stack.as_str());
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
        if let Some(bucket_type) = get_bucket_stack_type(client, name, stack).await {
            let bucket_name = Name::new(name)?;
            buckets.push(Bucket(bucket_name, bucket_type));
        }
    }

    Ok(buckets)
}

/// Get stack buckets filtered by type
pub async fn get_stack_buckets_by_type(
    client: &Client,
    stack: &StackName,
    types: &[Type],
) -> Result<Vec<Bucket>, RequestError> {
    let all_buckets = get_stack_buckets(client, stack).await?;
    Ok(all_buckets
        .into_iter()
        .filter(|b| types.contains(&b.1))
        .collect())
}

/// Check that user supplied bucket names are ok and make ready to create
pub fn review_bucket_names(
    config: &RequestConfig,
    names: &Vec<String>,
) -> Result<Vec<(Bucket, Bucket)>, RequestError> {
    let mut buckets: Vec<(Bucket, Bucket)> = Vec::new();

    for name in names {
        let bucket = Name::new(name)?;
        let primary = Request::primary_bucket(&config.stack, &bucket)?;
        let replication = Request::replication_bucket(&config.stack, &bucket)?;
        buckets.push((primary, replication));
    }

    Ok(buckets)
}

#[derive(Debug, PartialEq)]
pub struct Bucket(pub Name, pub Type);

impl Bucket {
    pub fn bucket_type(&self) -> &Type {
        &self.1
    }

    pub fn name(&self) -> &str {
        self.0.as_str()
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

        if name.starts_with("-") || name.ends_with("-") {
            return Err(RequestError::ValidationError(format!(
                "name cannot start or end with dash ({})",
                name
            )));
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
    pub fn primary_bucket(stack: &StackName, partial: &Name) -> Result<Bucket, RequestError> {
        if Self::uses_reserved_prefix_or_suffix(stack.as_str(), partial.as_str()) {
            return Err(RequestError::ValidationError(format!(
                "cannot use reserved prefix or suffix ({})",
                partial.as_str()
            )));
        }

        let name = Name::new(&format!("{}-{}", stack.as_str(), partial.as_str()))?;
        if name.as_str().ends_with(PUBLIC_SUFFIX) {
            Ok(Bucket(name, Type::Public))
        } else {
            Ok(Bucket(name, Type::Standard))
        }
    }

    pub fn replication_bucket(stack: &StackName, partial: &Name) -> Result<Bucket, RequestError> {
        let name = Name::new(&format!(
            "{}-{}{}",
            stack.as_str(),
            partial.as_str(),
            REPLICATION_SUFFIX
        ))?;
        Ok(Bucket(name, Type::Replication))
    }

    // TODO: tidy this up
    fn uses_reserved_prefix_or_suffix(prefix: &str, name: &str) -> bool {
        name.starts_with(prefix)
            || name.ends_with(REPLICATION_SUFFIX)
            || name.ends_with(apputils::stack::MANAGED_SUFFIX)
            || name.ends_with(apputils::stack::REQUEST_SUFFIX)
    }
}

/// Configuration elements required for bucket creation and setup
#[derive(Debug)]
pub struct RequestConfig {
    pub account_id: String,
    pub debug_handler: bool,
    pub replication_role_arn: String,
    pub s3_client: aws_sdk_s3::Client,
    pub stack: StackName,
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
    Managed,
    Public,
    Replication,
    Request,
    Standard,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Managed => write!(f, "managed"),
            Type::Public => write!(f, "public"),
            Type::Replication => write!(f, "replication"),
            Type::Request => write!(f, "request"),
            Type::Standard => write!(f, "standard"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_client::TestClientBuilder;

    #[tokio::test]
    async fn test_get_bucket_names() {
        let content = "123\n456\n789\n234\n567\n890";
        let file = File::new("test-bucket".to_string(), "buckets.txt".to_string());
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain".to_string()))
            .build();

        let names = get_bucket_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), 5);
        assert_eq!(names[0], "123");
        assert_eq!(names[1], "456");
        assert_eq!(names[2], "789");
        assert_eq!(names[3], "234");
        assert_eq!(names[4], "567");
    }

    #[tokio::test]
    async fn test_get_bucket_names_exceeds_size_limit() {
        let content = "a".repeat((MAX_BUCKETS_REQUEST_FILE_SIZE + 1) as usize);
        let file = File::new("test-bucket".to_string(), "buckets.txt".to_string());
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

    #[test]
    fn test_review_bucket_names() {
        let stack = StackName::new("test-stack").unwrap();
        let client = TestClientBuilder::new().ok().build();
        let config = RequestConfig {
            account_id: "123456789".to_string(),
            debug_handler: false,
            replication_role_arn: "123456789".to_string(),
            s3_client: client,
            stack,
        };

        let names = vec!["example".to_string(), "data-public".to_string()];

        let result = review_bucket_names(&config, &names).unwrap();

        assert_eq!(result.len(), 2);

        // First bucket pair (standard)
        assert_eq!(result[0].0.0.as_str(), "test-stack-example");
        assert_eq!(result[0].0.1, Type::Standard);
        assert_eq!(result[0].1.0.as_str(), "test-stack-example-repl");
        assert_eq!(result[0].1.1, Type::Replication);

        // Second bucket pair (public)
        assert_eq!(result[1].0.0.as_str(), "test-stack-data-public");
        assert_eq!(result[1].0.1, Type::Public);
        assert_eq!(result[1].1.0.as_str(), "test-stack-data-public-repl");
        assert_eq!(result[1].1.1, Type::Replication);
    }

    #[test]
    fn test_name_new() {
        assert!(Name::new("").is_err());

        // ok
        assert_eq!(Name::new("test").unwrap().as_str(), "test");
        assert_eq!(Name::new("TEsT").unwrap().as_str(), "test");
        assert_eq!(Name::new("test-stack").unwrap().as_str(), "test-stack");
        assert_eq!(Name::new("test-stack-1").unwrap().as_str(), "test-stack-1");

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
    }

    #[test]
    fn test_request_primary_bucket_standard() {
        let stack = StackName::new("test-stack").unwrap();
        let standard = Name::new("example").unwrap();

        let result = Request::primary_bucket(&stack, &standard).unwrap();
        assert_eq!(result.0.as_str(), "test-stack-example");
        assert_eq!(result.1, Type::Standard);
    }

    #[test]
    fn test_request_primary_bucket_public() {
        let stack = StackName::new("test-stack").unwrap();
        let public = Name::new("example-public").unwrap();

        let result = Request::primary_bucket(&stack, &public).unwrap();
        assert_eq!(result.0.as_str(), "test-stack-example-public");
        assert_eq!(result.1, Type::Public);
    }

    #[test]
    fn test_request_primary_bucket_reserved_validation() {
        let test_cases = vec![
            "test-stack",
            "test-bucket-request",
            "test-managed",
            "test-repl",
        ];

        let stack = StackName::new("test-stack").unwrap();

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
        let stack = StackName::new("test-stack").unwrap();
        let standard = Name::new("example").unwrap();

        let result = Request::replication_bucket(&stack, &standard).unwrap();
        assert_eq!(result.0.as_str(), "test-stack-example-repl");
        assert_eq!(result.1, Type::Replication);
    }

    #[test]
    fn test_request_replication_bucket_public() {
        let stack = StackName::new("test-stack").unwrap();
        let public = Name::new("example-public").unwrap();

        let result = Request::replication_bucket(&stack, &public).unwrap();
        assert_eq!(result.0.as_str(), "test-stack-example-public-repl");
        assert_eq!(result.1, Type::Replication);
    }
}
