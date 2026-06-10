use thiserror::Error;

#[derive(Debug, Error)]
pub enum BucketValidationError {
    #[error("File size {actual} bytes exceeds maximum of {max} bytes")]
    FileTooLarge { actual: i64, max: i64 },
    #[error("Content Type error: must be a text file")]
    InvalidContentType,
    #[error("Validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug, Error)]
pub enum ProcessingError {
    #[cfg(feature = "duckdb")]
    #[error("DuckDB error: {0}")]
    DuckDB(#[from] duckdb::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
