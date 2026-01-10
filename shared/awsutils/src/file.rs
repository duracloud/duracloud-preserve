use crate::bucket::RequestError;
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
pub struct File {
    bucket: String,
    object: String,
}

impl File {
    pub fn new(bucket: String, object: String) -> Self {
        Self { bucket, object }
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
