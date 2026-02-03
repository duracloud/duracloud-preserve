use crate::{
    batch::{BatchError, ChecksumJobReceipt, get_batch_manifest},
    bucket::RequestError,
    config::RequestConfig,
    file::{self, File},
};

/// Generate a consolidated checksum report using batch compute checksum results
pub async fn perform(config: &RequestConfig, job_file: &File) -> Result<(), RequestError> {
    tracing::info!("Retrieving job file from S3");

    let bytes = file::download_bytes(&config.client, job_file).await?;
    let receipt: ChecksumJobReceipt = serde_json::from_slice(&bytes)
        .map_err(|e| RequestError::S3Error(format!("failed to parse receipt: {}", e)))?;

    let source =
        match get_batch_manifest(config, &receipt.source_bucket, &receipt.source_job_id).await {
            Ok(manifest) => manifest,
            Err(BatchError::NotReady(e)) => {
                tracing::info!("{}", e);
                return Ok(());
            }
            Err(e) => return Err(RequestError::S3Error(e.to_string())),
        };

    dbg!(source);

    let repl = match get_batch_manifest(config, &receipt.repl_bucket, &receipt.repl_job_id).await {
        Ok(manifest) => manifest,
        Err(BatchError::NotReady(e)) => {
            tracing::info!("{}", e);
            return Ok(());
        }
        Err(e) => return Err(RequestError::S3Error(e.to_string())),
    };

    dbg!(repl);

    // download the files (as inventory does)
    // pass to ChecksumVerifier::load(source_reports, replication_reports)

    Ok(())
}
