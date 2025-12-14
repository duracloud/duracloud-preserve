use crate::{
    bucket_creator::BucketCreator,
    file::{File, download},
};
use apputils::StackName;

use aws_sdk_s3::Client;
use tokio::io::AsyncBufReadExt;

pub const MAX_BUCKETS_PER_REQUEST: u8 = 5;
pub const MAX_BUCKETS_REQUEST_FILE_SIZE: u8 = 32;
pub const REQ_BUCKETS_REQUEST_CONTENT_TYPE: &str = "text/plain";

const PUBLIC_SUFFIX: &str = "-public";
const REPLICATION_SUFFIX: &str = "-repl";

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

/// Retrieve bucket request file and check is valid
pub async fn get_bucket_names(client: &Client, file: &File) -> Result<Vec<String>, RequestError> {
    let Ok(r) = download(&client, &file).await else {
        return Err(RequestError::S3Error("failed to download file".to_string()));
    };

    if let Some(ct) = r.content_type()
        && ct != REQ_BUCKETS_REQUEST_CONTENT_TYPE
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

/// A type wrapper to ensure bucket name is compatible with
/// S3 and project requirements.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name(String);

impl Name {
    pub fn new(name: &str) -> Result<Self, RequestError> {
        let name = name.to_lowercase();

        if name.starts_with("-") || name.ends_with("-") {
            return Err(RequestError::ValidationError(
                "name cannot start or end with dash".to_string(),
            ));
        }

        // TODO length
        // TODO valid characters

        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Handles conversion of a stack name + user requested bucket
/// name to a full length S3 primary and replication bucket name.
#[derive(Debug)]
pub struct Request {}

impl Request {
    pub fn primary_bucket(stack: &StackName, partial: &Name) -> Result<Bucket, RequestError> {
        if partial.as_str().starts_with(stack.as_str()) {
            return Err(RequestError::ValidationError(
                "duplicated stack name in bucket request name".to_string(),
            ));
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
#[derive(Debug)]
pub enum RequestError {
    FileTooLarge { actual: i64, max: i64 },
    InvalidContentType,
    IoError(std::io::Error),
    S3Error(String),
    ValidationError(String),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::FileTooLarge { actual, max } => write!(
                f,
                "File size {} bytes exceeds maximum of {} bytes",
                actual, max
            ),
            RequestError::InvalidContentType => {
                write!(f, "Content Type error: must be a text file")
            }
            RequestError::IoError(e) => write!(f, "IO error: {}", e),
            RequestError::S3Error(msg) => write!(f, "S3 error: {}", msg),
            RequestError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for RequestError {}

/// Types for buckets
#[derive(Debug, PartialEq)]
pub enum Type {
    Public,
    Replication,
    Standard,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Public => write!(f, "public"),
            Type::Replication => write!(f, "replication"),
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
        let file = File::new("test-bucket".to_string(), "files/buckets.txt".to_string());
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
        let file = File::new("test-bucket".to_string(), "files/buckets.txt".to_string());
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
        assert_eq!(Name::new("test").unwrap().as_str(), "test");
        assert_eq!(Name::new("TEsT").unwrap().as_str(), "test");
        assert!(Name::new("-test").is_err());
        assert!(Name::new("test-").is_err());
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
    fn test_request_primary_bucket_error() {
        let stack = StackName::new("test-stack").unwrap();
        let public = Name::new("test-stack-example").unwrap();

        let result = Request::primary_bucket(&stack, &public);

        assert!(result.is_err());
        match result.unwrap_err() {
            RequestError::ValidationError(msg) => {
                assert_eq!(msg, "duplicated stack name in bucket request name");
            }
            _ => panic!("Expected ValidationError"),
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
