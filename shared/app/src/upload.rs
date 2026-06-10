use std::path::Path;

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

/// Upload content to the dated contextualized path, then server-side copy it
/// to the `0000-00-00-LATEST` sentinel path (the version) which is updated
/// each time.
pub async fn put_versioned_bytes<E>(
    config: &Config,
    date_ctx: DateCtx,
    body: impl Into<Bytes>,
    content_type: &str,
    path_for_ctx: impl Fn(DateCtx) -> ManagedFile,
    map_err: impl Fn(RequestError) -> E,
) -> Result<(), E> {
    let dated = File::from(path_for_ctx(date_ctx));
    put_bytes(config.s3(), body, content_type, [dated.clone()])
        .await
        .map_err(&map_err)?;
    copy_to_latest(config, &dated, &path_for_ctx)
        .await
        .map_err(map_err)
}

/// Upload a local file to the dated contextualized path by streaming it from
/// disk, then server-side copy it to the `0000-00-00-LATEST` sentinel path.
pub async fn put_versioned_file<E>(
    config: &Config,
    date_ctx: DateCtx,
    path: &Path,
    content_type: &str,
    path_for_ctx: impl Fn(DateCtx) -> ManagedFile,
    map_err: impl Fn(RequestError) -> E,
) -> Result<(), E> {
    let dated = File::from(path_for_ctx(date_ctx));
    tracing::info!("Uploading: {}", dated.s3_url());
    file::upload_path(config.s3(), &dated, path, content_type)
        .await
        .map_err(&map_err)?;
    copy_to_latest(config, &dated, &path_for_ctx)
        .await
        .map_err(map_err)
}

/// Copy the dated upload to the LATEST sentinel key server-side, skipping the
/// self-copy when the dated path already is the LATEST path.
async fn copy_to_latest(
    config: &Config,
    dated: &File,
    path_for_ctx: &impl Fn(DateCtx) -> ManagedFile,
) -> Result<(), RequestError> {
    let latest = File::from(path_for_ctx(DateCtx::Latest));
    if dated.key() == latest.key() {
        return Ok(());
    }

    tracing::info!("Copying {} to {}", dated.s3_url(), latest.s3_url());
    file::copy(config.s3(), dated, &latest).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config as app_config;
    use constants::TEXT_CSV;
    use test_support::{TestClientBuilder, recorded_requests};

    const COPY_RESULT_XML: &str = r#"<CopyObjectResult><ETag>"etag"</ETag></CopyObjectResult>"#;
    const SOURCE_BUCKET: &str = "test-stack-private";

    fn uri_has_key(uri: &str, key: &str) -> bool {
        let encoded_upper = key.replace('/', "%2F");
        let encoded_lower = key.replace('/', "%2f");
        uri.contains(key) || uri.contains(&encoded_upper) || uri.contains(&encoded_lower)
    }

    #[tokio::test]
    async fn test_put_versioned_bytes_puts_dated_then_copies_to_latest() {
        let (sdk_config, replay) = TestClientBuilder::new()
            .ok()
            .success(COPY_RESULT_XML, None)
            .build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);

        put_versioned_bytes(
            &config,
            DateCtx::Today,
            "a,b\n",
            TEXT_CSV,
            |ctx| config.stack().reports_manifests_path(SOURCE_BUCKET, ctx),
            |e| e,
        )
        .await
        .unwrap();

        let dated = config
            .stack()
            .reports_manifests_path(SOURCE_BUCKET, DateCtx::Today);
        let latest = config
            .stack()
            .reports_manifests_path(SOURCE_BUCKET, DateCtx::Latest);

        let requests = recorded_requests(&replay);
        assert_eq!(requests.len(), 2);

        assert_eq!(requests[0].method, "PUT");
        assert!(uri_has_key(&requests[0].uri, dated.key()));
        assert!(requests[0].copy_source.is_none());
        assert_eq!(requests[0].body, b"a,b\n");

        assert_eq!(requests[1].method, "PUT");
        assert!(uri_has_key(&requests[1].uri, latest.key()));
        let copy_source = requests[1].copy_source.as_deref().unwrap();
        assert!(copy_source.ends_with(dated.key()), "got: {copy_source}");
        assert!(requests[1].body.is_empty());
    }

    #[tokio::test]
    async fn test_put_versioned_bytes_skips_copy_for_latest_ctx() {
        let (sdk_config, replay) = TestClientBuilder::new().ok().build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);

        put_versioned_bytes(
            &config,
            DateCtx::Latest,
            "a,b\n",
            TEXT_CSV,
            |ctx| config.stack().reports_manifests_path(SOURCE_BUCKET, ctx),
            |e| e,
        )
        .await
        .unwrap();

        let requests = recorded_requests(&replay);
        assert_eq!(requests.len(), 1);
        assert!(requests[0].copy_source.is_none());
    }

    #[tokio::test]
    async fn test_put_versioned_file_streams_file_then_copies_to_latest() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("report.csv");
        std::fs::write(&path, "bucket,key\nb,k\n").unwrap();

        let (sdk_config, replay) = TestClientBuilder::new()
            .ok()
            .success(COPY_RESULT_XML, None)
            .build_sdk_config_with_replay();
        let config = app_config::Config::for_tests(sdk_config, false);

        put_versioned_file(
            &config,
            DateCtx::Today,
            &path,
            TEXT_CSV,
            |ctx| config.stack().reports_manifests_path(SOURCE_BUCKET, ctx),
            |e| e,
        )
        .await
        .unwrap();

        let dated = config
            .stack()
            .reports_manifests_path(SOURCE_BUCKET, DateCtx::Today);
        let latest = config
            .stack()
            .reports_manifests_path(SOURCE_BUCKET, DateCtx::Latest);

        let requests = recorded_requests(&replay);
        assert_eq!(requests.len(), 2);

        assert_eq!(requests[0].method, "PUT");
        assert!(uri_has_key(&requests[0].uri, dated.key()));
        assert_eq!(requests[0].content_type.as_deref(), Some(TEXT_CSV));
        assert!(requests[0].copy_source.is_none());

        assert_eq!(requests[1].method, "PUT");
        assert!(uri_has_key(&requests[1].uri, latest.key()));
        let copy_source = requests[1].copy_source.as_deref().unwrap();
        assert!(copy_source.ends_with(dated.key()), "got: {copy_source}");
    }
}
