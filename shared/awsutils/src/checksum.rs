use crate::{
    batch::BatchResultEntry,
    bucket::RequestError,
    file::{self, File},
};

pub use base::checksum::{ChecksumVerifier, process};

pub async fn download_manifest_files(
    client: &aws_sdk_s3::Client,
    results: Vec<BatchResultEntry>,
    temp_dir: &tempfile::TempDir,
) -> Result<Vec<String>, RequestError> {
    let files = results
        .into_iter()
        .filter_map(|entry| {
            if !entry
                .task_execution_status
                .eq_ignore_ascii_case("succeeded")
            {
                tracing::warn!(
                    task_execution_status = %entry.task_execution_status,
                    bucket = %entry.bucket,
                    key = %entry.key,
                    "Skipping batch result with non-succeeded task status",
                );
                return None;
            }

            Some(File::new(entry.bucket, entry.key))
        })
        .collect::<Vec<_>>();

    let local_paths =
        file::download_files_to_temp(client, &files, temp_dir, "batch manifest result").await?;

    Ok(local_paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}
