use aws_sdk_s3::{
    Client,
    config::{Credentials, Region},
    error::SdkError,
    operation::get_object::{GetObjectError, GetObjectOutput},
    primitives::SdkBody,
};
use aws_smithy_runtime::client::http::test_util::{ReplayEvent, StaticReplayClient};

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

/// Provide a simple preconfigured test client
pub fn test_client(uri: String, body: SdkBody, content_length: Option<i64>) -> Client {
    let mut response_builder = http::Response::builder().status(200);

    if let Some(length) = content_length {
        response_builder = response_builder.header("Content-Length", length.to_string());
    }

    let http_client = StaticReplayClient::new(vec![ReplayEvent::new(
        http::Request::builder()
            .uri(uri)
            .body(SdkBody::empty())
            .unwrap(),
        response_builder.body(body).unwrap(),
    )]);

    let config = aws_sdk_s3::Config::builder()
        .behavior_version_latest()
        .http_client(http_client)
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::new("test", "test", None, None, "test"))
        .build();

    aws_sdk_s3::Client::from_conf(config)
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
