use apputils::stats::VerificationStats;
use thiserror::Error;

use crate::{
    batch::BatchResultEntry,
    bucket::RequestError,
    file::{self, File},
};

pub use apputils::checksum::{ChecksumError as ProcessingError, ChecksumVerifier, process};

#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Processing error: {0}")]
    Processing(#[from] ProcessingError),
    #[error("{0}")]
    Request(#[from] RequestError),
}

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

pub fn empty_stats() -> VerificationStats {
    VerificationStats {
        total_objects: 0,
        matches: 0,
        mismatches: 0,
        missing_replica: 0,
        missing_source: 0,
        failed_source: 0,
        failed_replication: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_stats_shape() {
        let stats = empty_stats();
        assert_eq!(stats.total_objects, 0);
        assert_eq!(stats.matches, 0);
        assert_eq!(stats.mismatches, 0);
        assert_eq!(stats.missing_replica, 0);
        assert_eq!(stats.missing_source, 0);
        assert_eq!(stats.failed_source, 0);
        assert_eq!(stats.failed_replication, 0);
    }
}
