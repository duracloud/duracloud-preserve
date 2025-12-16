use aws_sdk_s3::{
    Client,
    error::SdkError,
    operation::get_object::{GetObjectError, GetObjectOutput},
};

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

/// Make put object request for file
pub fn upload(_: File) {
    todo!()
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

    pub fn http_url(&self) -> String {
        format!("https://{}.s3.amazonaws.com/{}", self.bucket, self.object)
    }

    pub fn s3_url(&self) -> String {
        format!("s3://{}/{}", self.bucket, self.object)
    }
}
