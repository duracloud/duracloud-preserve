use crate::{
    batch::{ChecksumJobReceipt, get_manifest_if_ready},
    bucket::RequestError,
    config::{BatchConfig, RequestConfig},
    file::{self, File},
};

/// Generate a consolidated checksum report using batch compute checksum results
pub async fn perform(
    batch: &BatchConfig,
    request: &RequestConfig,
    job_file: &File,
) -> Result<(), RequestError> {
    tracing::info!("Retrieving job file from S3: {}", job_file.s3_url());

    let bytes = file::download_bytes(&request.client, job_file).await?;
    let receipt: ChecksumJobReceipt = serde_json::from_slice(&bytes)
        .map_err(|e| RequestError::S3Error(format!("failed to parse receipt: {}", e)))?;

    let Some(source) = get_manifest_if_ready(
        batch,
        request,
        &receipt.source_bucket,
        &receipt.source_job_id,
    )
    .await?
    else {
        return Ok(());
    };

    dbg!(source);

    let Some(repl) =
        get_manifest_if_ready(batch, request, &receipt.repl_bucket, &receipt.repl_job_id).await?
    else {
        return Ok(());
    };

    dbg!(repl);

    // download the files (as inventory does)
    // pass to ChecksumVerifier::load(source_reports, replication_reports)

    Ok(())
}
