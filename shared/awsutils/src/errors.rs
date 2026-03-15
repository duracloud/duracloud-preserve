use apputils::errors::BucketValidationError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Processing error: {0}")]
    Processing(#[from] apputils::errors::ChecksumError),
    #[error("{0}")]
    Request(#[from] RequestError),
}

/// Shared AWS utility error type used across config, file, and bucket operations.
#[derive(Debug, Error)]
pub enum RequestError {
    #[error("Config error: {0}")]
    ConfigError(String),
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

impl From<BucketValidationError> for RequestError {
    fn from(value: BucketValidationError) -> Self {
        match value {
            BucketValidationError::ValidationError(message) => Self::ValidationError(message),
            BucketValidationError::FileTooLarge { actual, max } => {
                Self::FileTooLarge { actual, max }
            }
            BucketValidationError::InvalidContentType => Self::InvalidContentType,
        }
    }
}

/// Wrap s3 errors with a contextual message.
pub trait S3ResultExt<T> {
    fn s3_err(self, context: impl Into<String>) -> Result<T, RequestError>;
}

impl<T, E: std::fmt::Display> S3ResultExt<T> for Result<T, E> {
    fn s3_err(self, context: impl Into<String>) -> Result<T, RequestError> {
        self.map_err(|e| RequestError::S3Error(format!("{}: {e}", context.into())))
    }
}
