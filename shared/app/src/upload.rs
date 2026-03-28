use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use awsutils::{
    bucket::RequestError,
    file::{self, File},
};
use base::{ManagedFile, stack::DateCtx};
use bytes::Bytes;
use constants::TEXT_PLAIN;
use futures::future;

use crate::config::Config;

/// Upload bytes to one or more S3 files concurrently, returning their HTTP URLs.
pub async fn put_bytes(
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
    future::try_join_all(uploads).await
}

/// Upload content to the stack managed bucket feedback path.
pub async fn put_feedback(config: &Config, key: &str, message: String) {
    let file = File::from(config.stack().feedback_path(key));
    if let Err(e) = put_bytes(config.s3(), message.into_bytes(), TEXT_PLAIN, [file]).await {
        tracing::error!("Failed to upload feedback: {e}");
    }
}

/// Upload content to dated contextualized paths and to
/// "latest" (the version) which is updated each time.
pub async fn put_versioned_bytes<E>(
    config: &Config,
    date_ctx: DateCtx,
    body: impl Into<Bytes>,
    content_type: &str,
    path_for_ctx: impl Fn(DateCtx) -> ManagedFile,
    map_err: impl Fn(RequestError) -> E,
) -> Result<(), E> {
    let files = [DateCtx::Latest, date_ctx].map(|ctx| File::from(path_for_ctx(ctx)));
    put_bytes(config.s3(), body, content_type, files)
        .await
        .map(|_| ())
        .map_err(map_err)
}
