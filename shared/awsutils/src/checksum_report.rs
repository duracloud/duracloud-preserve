use crate::{
    batch::{ChecksumJobReceipt, get_manifest_if_ready},
    bucket::RequestError,
    config::Config,
    file::{self, File},
};

#[derive(Debug, Default)]
pub struct PerformOptions {}

/// Generate a consolidated checksum report using batch compute checksum results
pub async fn perform(
    config: &Config,
    job_file: &File,
    _opts: &PerformOptions,
) -> Result<(), RequestError> {
    tracing::info!("Retrieving job receipt from S3: {}", job_file.s3_url());

    let bytes = file::download_bytes(config.s3(), job_file).await?;
    let receipt: ChecksumJobReceipt = serde_json::from_slice(&bytes)?;

    let Some(source) =
        get_manifest_if_ready(config, &receipt.source_bucket, &receipt.source_job_id).await?
    else {
        tracing::info!("Source job {} not ready yet", receipt.source_job_id);
        return Ok(());
    };

    tracing::info!("Source job file found: {:?}", &source);

    let Some(repl) =
        get_manifest_if_ready(config, &receipt.repl_bucket, &receipt.repl_job_id).await?
    else {
        tracing::info!("Replication job {} not ready yet", receipt.repl_job_id);
        return Ok(());
    };

    tracing::info!("Replication job file found: {:?}", &repl);

    // download the files (as inventory does)
    // pass to ChecksumVerifier::load(source_reports, replication_reports)

    Ok(())
}
