use crate::bucket::RequestError;
use crate::errors::S3ResultExt;
use aws_sdk_s3::{
    Client,
    error::{ProvideErrorMetadata, SdkError},
    operation::get_object::{GetObjectError, GetObjectOutput},
    operation::head_object::{HeadObjectError, HeadObjectOutput},
    primitives::ByteStream,
};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Server-side copy within S3 (the data does not round-trip through the client).
pub async fn copy(client: &Client, src: &File, dest: &File) -> Result<(), RequestError> {
    client
        .copy_object()
        .bucket(&dest.bucket)
        .key(&dest.object)
        .copy_source(format!("{}/{}", src.bucket, src.object))
        .send()
        .await
        .s3_err("failed to copy file")?;

    Ok(())
}

/// Delete a file from S3
pub async fn delete(client: &Client, file: &File) -> Result<(), RequestError> {
    client
        .delete_object()
        .bucket(&file.bucket)
        .key(&file.object)
        .send()
        .await
        .s3_err("failed to delete file")?;

    Ok(())
}

/// Make get object request for file
pub async fn download(
    client: &Client,
    file: &File,
) -> Result<GetObjectOutput, SdkError<GetObjectError>> {
    client
        .get_object()
        .bucket(&file.bucket)
        .key(&file.object)
        .send()
        .await
}

/// Download file content as bytes
pub async fn download_bytes(client: &Client, file: &File) -> Result<bytes::Bytes, RequestError> {
    let response = download(client, file)
        .await
        .s3_err("failed to download file")?;

    response
        .body
        .collect()
        .await
        .s3_err("failed to read body")
        .map(|data| data.into_bytes())
}

/// Download S3 files to a temporary directory using collision-safe local names.
pub async fn download_files_to_temp(
    client: &Client,
    files: &[File],
    temp_dir: &tempfile::TempDir,
    file_kind: &str,
) -> Result<Vec<PathBuf>, RequestError> {
    let mut local_paths = Vec::new();

    for (index, file) in files.iter().enumerate() {
        tracing::info!(
            file_kind,
            s3_url = %file.s3_url(),
            "Downloading file from S3",
        );

        let filename = file.key().rsplit('/').next().unwrap_or(file.key());
        let local_path = temp_dir.path().join(format!("{index:05}-{filename}"));

        let mut response = download(client, file)
            .await
            .s3_err("failed to download file")?;
        let mut output = tokio::fs::File::create(&local_path).await?;
        while let Some(chunk) = response.body.next().await {
            let chunk = chunk.s3_err("failed to read body")?;
            output.write_all(&chunk).await?;
        }
        output.flush().await?;

        local_paths.push(local_path);
    }

    Ok(local_paths)
}

/// Check if a file exists in S3.
/// Returns `Ok(false)` when the object is absent (404/NotFound); other failures
/// (network, permissions, throttling) surface as `Err`.
pub async fn exists(client: &Client, file: &File) -> Result<bool, RequestError> {
    match client
        .head_object()
        .bucket(&file.bucket)
        .key(&file.object)
        .send()
        .await
    {
        Ok(_) => Ok(true),
        Err(e) if is_missing_object(&e) => Ok(false),
        Err(e) => Err(e).s3_err(format!("failed to head {}", file.s3_url())),
    }
}

fn is_missing_object(e: &SdkError<HeadObjectError>) -> bool {
    match e {
        SdkError::ServiceError(service) => {
            matches!(service.err(), HeadObjectError::NotFound(_))
                || service
                    .err()
                    .code()
                    .is_some_and(|code| matches!(code, "NotFound" | "NoSuchKey"))
        }
        _ => false,
    }
}

/// Head object request for file metadata (with checksums enabled)
pub async fn head(
    client: &Client,
    file: &File,
) -> Result<HeadObjectOutput, SdkError<HeadObjectError>> {
    use aws_sdk_s3::types::ChecksumMode;

    client
        .head_object()
        .bucket(&file.bucket)
        .key(&file.object)
        .checksum_mode(ChecksumMode::Enabled)
        .send()
        .await
}

/// Upload content to S3
pub async fn upload(
    client: &Client,
    file: &File,
    body: impl Into<ByteStream>,
    content_type: &str,
) -> Result<(), RequestError> {
    client
        .put_object()
        .bucket(&file.bucket)
        .key(&file.object)
        .body(body.into())
        .content_type(content_type)
        .send()
        .await
        .s3_err("failed to upload file")?;

    Ok(())
}

/// Upload a local file to S3 by streaming it from disk.
pub async fn upload_path(
    client: &Client,
    file: &File,
    path: &Path,
    content_type: &str,
) -> Result<(), RequestError> {
    let body = ByteStream::from_path(path)
        .await
        .map_err(|e| RequestError::from(std::io::Error::other(e)))?;

    upload(client, file, body, content_type).await
}

/// Basic type wrapper for an S3 "file" (bucket + key)
#[derive(Debug, Clone)]
pub struct File {
    bucket: String,
    object: String,
}

impl File {
    pub fn new(bucket: impl Into<String>, object: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            object: object.into(),
        }
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn http_url(&self) -> String {
        format!("https://{}.s3.amazonaws.com/{}", self.bucket, self.object)
    }

    pub fn key(&self) -> &str {
        &self.object
    }

    pub fn s3_url(&self) -> String {
        format!("s3://{}/{}", self.bucket, self.object)
    }
}

impl From<base::ManagedFile> for File {
    fn from(mf: base::ManagedFile) -> Self {
        File::new(mf.bucket(), mf.key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::{TestClientBuilder, recorded_requests};

    fn test_file() -> File {
        File::new("test-bucket", "missing.csv")
    }

    #[tokio::test]
    async fn test_copy_sends_copy_source_header() {
        let (client, replay) = TestClientBuilder::new()
            .success(
                r#"<CopyObjectResult><ETag>"etag"</ETag></CopyObjectResult>"#,
                None,
            )
            .build_with_replay();

        let src = File::new("src-bucket", "reports/2026-06-09-report.csv");
        let dest = File::new("dest-bucket", "reports/0000-00-00-LATEST-report.csv");
        copy(&client, &src, &dest).await.unwrap();

        let requests = recorded_requests(&replay);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "PUT");
        assert!(requests[0].uri.contains("dest-bucket"));
        assert!(
            requests[0]
                .uri
                .contains("reports/0000-00-00-LATEST-report.csv")
        );
        assert_eq!(
            requests[0].copy_source.as_deref(),
            Some("src-bucket/reports/2026-06-09-report.csv")
        );
    }

    #[tokio::test]
    async fn test_upload_path_streams_file_and_sets_content_type() {
        let (client, replay) = TestClientBuilder::new().ok().build_with_replay();

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("report.csv");
        tokio::fs::write(&path, b"bucket,key\nb,k\n").await.unwrap();

        let file = File::new("test-bucket", "report.csv");
        upload_path(&client, &file, &path, "text/csv")
            .await
            .unwrap();

        let requests = recorded_requests(&replay);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "PUT");
        assert_eq!(requests[0].content_type.as_deref(), Some("text/csv"));
    }

    #[tokio::test]
    async fn test_download_files_to_temp_streams_bytes_to_disk() {
        let content = "bucket,key,size\ntest,a.txt,1\ntest,b.txt,2\n";
        let client = TestClientBuilder::new().success(content, None).build();

        let temp_dir = tempfile::tempdir().unwrap();
        let files = [File::new("test-bucket", "manifests/data.csv")];
        let paths = download_files_to_temp(&client, &files, &temp_dir, "test")
            .await
            .unwrap();

        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("00000-data.csv"));
        let written = tokio::fs::read_to_string(&paths[0]).await.unwrap();
        assert_eq!(written, content);
    }

    #[tokio::test]
    async fn test_exists_returns_true_for_head_success() {
        let client = TestClientBuilder::new().ok().build();

        assert!(exists(&client, &test_file()).await.unwrap());
    }

    #[tokio::test]
    async fn test_exists_returns_false_for_not_found() {
        let client = TestClientBuilder::new()
            .error(404, "NotFound", "not found")
            .build();

        assert!(!exists(&client, &test_file()).await.unwrap());
    }

    #[tokio::test]
    async fn test_exists_returns_false_for_no_such_key() {
        let client = TestClientBuilder::new()
            .s3_error("NoSuchKey", "key not found")
            .build();

        assert!(!exists(&client, &test_file()).await.unwrap());
    }

    #[tokio::test]
    async fn test_exists_errors_for_access_denied() {
        let client = TestClientBuilder::new()
            .s3_error("AccessDenied", "forbidden")
            .build();

        let err = exists(&client, &test_file())
            .await
            .expect_err("access denied should not be treated as missing");
        assert!(err.to_string().contains("failed to head"));
    }
}
