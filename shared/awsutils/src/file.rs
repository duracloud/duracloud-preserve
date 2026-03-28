use crate::bucket::RequestError;
use crate::errors::S3ResultExt;
use aws_sdk_s3::{
    Client,
    error::SdkError,
    operation::get_object::{GetObjectError, GetObjectOutput},
    operation::head_object::{HeadObjectError, HeadObjectOutput},
    primitives::ByteStream,
};
use std::path::PathBuf;

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

        let bytes = download_bytes(client, file).await?;
        let filename = file.key().rsplit('/').next().unwrap_or(file.key());
        let local_path = temp_dir.path().join(format!("{index:05}-{filename}"));

        tokio::fs::write(&local_path, &bytes).await?;
        local_paths.push(local_path);
    }

    Ok(local_paths)
}

/// Check if a file exists in S3
pub async fn exists(client: &Client, file: &File) -> bool {
    client
        .head_object()
        .bucket(&file.bucket)
        .key(&file.object)
        .send()
        .await
        .is_ok()
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

/// Basic type wrapper for an S3 "file" (bucket + key)
#[derive(Debug)]
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
