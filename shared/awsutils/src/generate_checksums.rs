use crate::{batch::BatchError, config::BatchConfig};

/// Trigger S3 batch operation for generating checksum reports
pub async fn perform(_config: &BatchConfig) -> Result<(), BatchError> {
    tracing::info!("Retrieving buckets for checksum report");

    Ok(())
}
