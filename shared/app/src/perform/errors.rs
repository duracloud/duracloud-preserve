use apputils::bucket::BucketValidationError;
use awsutils::bucket::RequestError;
use thiserror::Error;
#[cfg(feature = "duckdb")]
use {crate::batch::BatchStatusError, awsutils::checksum};

#[derive(Debug, Error)]
pub enum BucketRequestError {
    #[error("failed to read bucket request file: {0}")]
    RequestFile(#[source] RequestError),
    #[error("invalid bucket request: {0}")]
    Validation(#[source] BucketValidationError),
    #[error("failed to create one or more buckets: {}", .0.join("; "))]
    CreateBuckets(Vec<String>),
    #[error("failed to delete request file: {0}")]
    Cleanup(#[source] RequestError),
}

#[derive(Debug, Error)]
pub enum ComputeChecksumsError {
    #[error("failed to discover buckets for checksum jobs: {0}")]
    BucketDiscovery(#[source] RequestError),
    #[error("invalid bucket: {0} (must be a standard or public bucket in the stack)")]
    InvalidBucket(String),
    #[error("failed to build replication bucket '{bucket}': {source}")]
    ReplicationBucket {
        bucket: String,
        #[source]
        source: BucketValidationError,
    },
    #[error("failed to pair source and replication buckets: {0}")]
    PairBuckets(#[source] BucketValidationError),
    #[error("failed to trigger checksum jobs for one or more buckets: {}", .0.join("; "))]
    PartialFailure(Vec<String>),
}

#[cfg(feature = "duckdb")]
#[derive(Debug, Error)]
pub enum ChecksumReportError {
    #[error("failed to download checksum receipt: {0}")]
    ReceiptDownload(#[source] RequestError),
    #[error("failed to parse checksum receipt: {0}")]
    ReceiptParse(#[source] serde_json::Error),
    #[error("failed to resolve batch manifests: {0}")]
    BatchStatus(#[source] BatchStatusError),
    #[error("failed to process checksum report: {0}")]
    Processing(#[source] checksum::ChecksumError),
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
    #[error("failed to upload file: {0}")]
    UploadError(#[source] RequestError),
}
