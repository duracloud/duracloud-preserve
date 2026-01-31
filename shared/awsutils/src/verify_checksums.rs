use crate::{
    batch::ChecksumJobReceipt,
    bucket::RequestError,
    config::RequestConfig,
    file::{self, File},
};

pub async fn perform(config: &RequestConfig, job_file: &File) -> Result<(), RequestError> {
    tracing::info!("Retrieving job file from S3");

    let bytes = file::download_bytes(&config.client, job_file).await?;
    let receipt: ChecksumJobReceipt = serde_json::from_slice(&bytes)
        .map_err(|e| RequestError::S3Error(format!("failed to parse receipt: {}", e)))?;

    dbg!(receipt);

    // construct File for source and replication batch manifests
    // download the manifests
    // download the files (as inventory does)
    // pass to ChecksumVerifier::load(source_reports, replication_reports)

    Ok(())
}
