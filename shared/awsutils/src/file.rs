use crate::{bucket::RequestError, config::Config};
use aws_sdk_s3::{
    Client,
    error::SdkError,
    operation::get_object::{GetObjectError, GetObjectOutput},
    primitives::ByteStream,
};
/// Delete a file from S3
pub async fn delete(client: &Client, file: &File) -> Result<(), RequestError> {
    client
        .delete_object()
        .bucket(&file.bucket)
        .key(&file.object)
        .send()
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to delete file: {}", e)))?;

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
        .map_err(|e| RequestError::S3Error(format!("failed to download file: {}", e)))?;

    response
        .body
        .collect()
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to read body: {}", e)))
        .map(|data| data.into_bytes())
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

/// Upload a file to the managed bucket feedback path
pub async fn feedback(
    config: &Config,
    key: &str,
    body: impl Into<ByteStream>,
    content_type: &str,
) -> Result<(), RequestError> {
    upload(
        config.s3(),
        &File::new(
            config.stack().managed_bucket(),
            config.stack().feedback_path(key),
        ),
        body,
        content_type,
    )
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
        .map_err(|e| RequestError::S3Error(format!("failed to upload file: {}", e)))?;

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
