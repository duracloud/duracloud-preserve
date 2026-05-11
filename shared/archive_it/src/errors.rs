use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArchiveItError {
    #[error("Archive-It client error: {0}")]
    Client(#[from] archive_it_client::Error),
    #[error("Archive-It resource not found: {0}")]
    NotFound(String),
    #[error("AWS S3 error: {0}")]
    S3(#[from] awsutils::bucket::RequestError),
}
