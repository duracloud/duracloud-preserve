use apputils::{content_type::TEXT_PLAIN, stack::DateCtx};
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use awsutils::{
    bucket::RequestError,
    file::{self, File},
};
use bytes::Bytes;
use futures::future::try_join_all;

use crate::config::Config;

/// Upload bytes to one or more S3 files concurrently, returning their HTTP URLs.
pub async fn upload_bytes(
    client: &Client,
    body: impl Into<Bytes>,
    content_type: &str,
    files: impl IntoIterator<Item = File>,
) -> Result<Vec<String>, RequestError> {
    let bytes: Bytes = body.into();
    let uploads = files.into_iter().map(|file| {
        let stream = ByteStream::from(bytes.clone());
        async move {
            tracing::info!("Uploading: {}", file.s3_url());
            file::upload(client, &file, stream, content_type).await?;
            Ok::<_, RequestError>(file.http_url())
        }
    });
    try_join_all(uploads).await
}

pub async fn upload_feedback(config: &Config, key: &str, message: String) {
    let stack = config.stack();
    let file = File::new(stack.managed_bucket(), stack.feedback_path(key));
    if let Err(e) = upload_bytes(config.s3(), message.into_bytes(), TEXT_PLAIN, [file]).await {
        tracing::error!("Failed to upload feedback: {e}");
    }
}

pub async fn upload_versioned_bytes<E>(
    config: &Config,
    date_ctx: DateCtx,
    body: impl Into<Bytes>,
    content_type: &str,
    path_for_ctx: impl Fn(DateCtx) -> String,
    map_err: impl Fn(RequestError) -> E,
) -> Result<(), E> {
    let bucket = config.stack().managed_bucket();
    let files = [DateCtx::Latest, date_ctx].map(|ctx| File::new(&bucket, path_for_ctx(ctx)));
    upload_bytes(config.s3(), body, content_type, files)
        .await
        .map(|_| ())
        .map_err(map_err)
}
