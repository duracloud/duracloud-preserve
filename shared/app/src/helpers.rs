use apputils::{content_type::TEXT_PLAIN, stack::DateCtx};
use aws_sdk_s3::primitives::ByteStream;
use aws_smithy_types::body::SdkBody;
use awsutils::{
    bucket::RequestError,
    file::{self, File},
};
use bytes::Bytes;

use crate::config::Config;

pub async fn upload_feedback_text(config: &Config, request_key: &str, message: impl Into<String>) {
    if let Err(fb_err) = file::feedback(
        config.s3(),
        config.stack(),
        request_key,
        SdkBody::from(message.into()),
        TEXT_PLAIN,
    )
    .await
    {
        tracing::error!("Failed to upload feedback: {fb_err}");
    }
}

pub async fn upload_versioned_bytes<E, PathForCtx, MapErr>(
    config: &Config,
    date_ctx: DateCtx,
    body: &Bytes,
    content_type: &str,
    upload_label: &str,
    path_for_ctx: PathForCtx,
    map_err: MapErr,
) -> Result<(), E>
where
    PathForCtx: Fn(DateCtx) -> String,
    MapErr: Fn(RequestError) -> E + Copy,
{
    let managed_bucket = config.stack().managed_bucket();

    for ctx in [DateCtx::Latest, date_ctx] {
        let output_file = File::new(&managed_bucket, path_for_ctx(ctx));

        tracing::info!("Uploading {upload_label}: {}", output_file.s3_url());

        file::upload(
            config.s3(),
            &output_file,
            ByteStream::from(body.clone()),
            content_type,
        )
        .await
        .map_err(map_err)?;
    }

    Ok(())
}
