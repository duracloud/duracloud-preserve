use awsutils::{batch::BatchError, bucket::RequestError};
use base::errors::BucketValidationError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BatchStatusError {
    #[error("{0}")]
    Batch(#[from] BatchError),
    #[error("Batch job failed: {0}")]
    JobFailed(String),
    #[error("Job matching id not found: {0}")]
    JobNotFound(String),
    #[error("Manifest not found: {0}")]
    ManifestNotFound(String),
    #[error("Job status matching id not found: {0}")]
    MissingStatus(String),
}

#[derive(Debug, Error)]
pub enum BucketRequestError {
    #[error("failed to delete request file: {0}")]
    Cleanup(#[source] RequestError),
    #[error("failed to create one or more buckets: {}", .0.join("; "))]
    CreateBuckets(Vec<String>),
    #[error("failed to read bucket request file: {0}")]
    RequestFile(#[source] RequestError),
    #[error("invalid bucket request: {0}")]
    Validation(#[source] BucketValidationError),
}

#[derive(Debug, Error)]
pub enum BucketReconciliationError {
    #[error("failed to discover buckets for reconciliation: {0}")]
    BucketDiscovery(#[from] RequestError),
    #[error("Drift detected: {0}")]
    DriftDetected(String),
}

#[derive(Debug, Error)]
pub enum ChecksumInventoryError {
    #[error("failed to parse inventory CSV: {0}")]
    CsvParse(#[from] csv::Error),
    #[error("failed to download inventory CSV: {0}")]
    Download(#[source] RequestError),
    #[error("cannot extract bucket name from inventory file key: {0}")]
    InvalidFileKey(#[from] FileKeyError),
    #[error("failed to upload checksum inventory: {0}")]
    Upload(#[source] RequestError),
}

#[cfg(feature = "duckdb")]
#[derive(Debug, Error)]
pub enum ChecksumReportError {
    #[error("failed to resolve batch manifests: {0}")]
    BatchStatus(#[from] BatchStatusError),
    #[error("failed to process checksum report: {0}")]
    Processing(String),
    #[error("failed to download checksum receipt: {0}")]
    ReceiptDownload(#[from] RequestError),
    #[error("failed to parse checksum receipt: {0}")]
    ReceiptParse(#[from] serde_json::Error),
    #[error("failed to generate temporary file: #{0}")]
    TempFileNotCreated(#[from] std::io::Error),
    #[error("failed to upload checksum report: {0}")]
    Upload(#[source] RequestError),
}

#[cfg(feature = "duckdb")]
impl From<base::errors::ProcessingError> for ChecksumReportError {
    fn from(value: base::errors::ProcessingError) -> Self {
        ChecksumReportError::Processing(value.to_string())
    }
}

#[derive(Debug, Error)]
pub enum ComputeChecksumsError {
    #[error("failed to discover buckets for checksum jobs: {0}")]
    BucketDiscovery(#[source] RequestError),
    #[error("invalid bucket: {0} (must be a standard or public bucket in the stack)")]
    InvalidBucket(String),
    #[error("failed to pair source and replication buckets: {0}")]
    PairBuckets(#[source] BucketValidationError),
    #[error("failed to trigger checksum jobs for one or more buckets: {}", .0.join("; "))]
    PartialFailure(Vec<String>),
    #[error("failed to build replication bucket '{bucket}': {source}")]
    ReplicationBucket {
        bucket: String,
        #[source]
        source: BucketValidationError,
    },
}

#[derive(Debug, Error)]
pub enum FileKeyError {
    #[error("file key has no extension: {0}")]
    MissingExtension(String),
}

#[cfg(feature = "duckdb")]
#[derive(Debug, Error)]
pub enum InventoryReportError {
    #[error("failed to download inventory files: {0}")]
    Download(#[source] RequestError),
    #[error("invalid inventory format: expected '{expected}', got '{actual}'")]
    InvalidFormat { expected: String, actual: String },
    #[error("failed to create temporary directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to serialize inventory stats: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to fetch inventory manifest: {0}")]
    ManifestFetch(String),
    #[error("failed to process inventory: {0}")]
    Processing(#[from] base::errors::ProcessingError),
    #[error("failed to upload inventory report: {0}")]
    Upload(#[source] RequestError),
}

#[cfg(feature = "duckdb")]
impl From<awsutils::errors::InventoryError> for InventoryReportError {
    fn from(value: awsutils::errors::InventoryError) -> Self {
        InventoryReportError::ManifestFetch(value.to_string())
    }
}

#[derive(Debug, Error)]
pub enum StorageReportError {
    #[error("failed to discover buckets for storage report: {0}")]
    BucketDiscovery(#[source] RequestError),
    #[error("failed to download inventory stats for bucket '{bucket}': {source}")]
    DownloadStats {
        bucket: String,
        #[source]
        source: RequestError,
    },
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to parse inventory stats for bucket '{bucket}': {source}")]
    ParseStats {
        bucket: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to render storage report HTML: {0}")]
    Render(#[from] askama::Error),
    #[error("failed to upload file: {0}")]
    UploadError(#[source] RequestError),
}

#[derive(Debug, Error)]
pub enum SyncUsersError {
    #[error("failed to list buckets for stack: {0}")]
    BucketDiscovery(#[from] RequestError),
    #[error("failed to retrieve iam data: {0}")]
    IamError(String),
    #[error("SFTPGo error: {0}")]
    SftpGo(#[from] sftpgo::Error),
    #[error("failed to retrieve credentials for user '{user_name}': {source}")]
    UserCredentials {
        user_name: String,
        #[source]
        source: RequestError,
    },
    #[error("failed to find eligible users")]
    UserDiscovery,
}

impl From<aws_sdk_iam::Error> for SyncUsersError {
    fn from(value: aws_sdk_iam::Error) -> Self {
        SyncUsersError::IamError(value.to_string())
    }
}
