use app::{
    config::Config,
    perform::bucket_request::{self, PerformArgs},
};
use aws_lambda_events::event::s3::S3Event;
use aws_sdk_s3::types::TransitionStorageClass;
use awsutils::{bucket_creator, file::File};
use constants::BUCKET_REQUEST_PREFIX;
use lambda_runtime::{Error, LambdaEvent, tracing};

#[derive(Debug, Clone)]
pub(crate) struct HandlerOptions {
    pub standard_storage_tier: TransitionStorageClass,
}

impl Default for HandlerOptions {
    fn default() -> Self {
        Self {
            standard_storage_tier: bucket_creator::STORAGE_CLASS_STANDARD_DEFAULT,
        }
    }
}

pub(crate) async fn function_handler(
    config: &Config,
    handler_opts: &HandlerOptions,
    event: LambdaEvent<S3Event>,
) -> Result<(), Error> {
    let payload = event.payload;

    let record = payload.records.first().expect("payload should have record");
    let bucket = record.s3.bucket.name.as_ref().expect("bucket required");
    let object = record.s3.object.key.as_ref().expect("object requried");

    tracing::info!("Bucket: {:?}, Object: {:?}", bucket, object);

    if bucket != &config.stack().request_bucket() || !object.starts_with(BUCKET_REQUEST_PREFIX) {
        panic!(
            "Not the request bucket or path for this stack: {:?}",
            config.stack()
        );
    }

    if config.debug_handler() {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    let args = PerformArgs {
        request_file: File::new(bucket, object),
        standard_storage_tier: handler_opts.standard_storage_tier.clone(),
        trigger_sync_users: true,
    };

    bucket_request::perform(config, &args)
        .await
        .map_err(|e| Error::from(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::config as app_config;
    use lambda_runtime::{Context, LambdaEvent};
    use test_support::TestClientBuilder;

    #[tokio::test]
    #[should_panic(expected = "Not the request bucket or path for this stack")]
    async fn test_invalid_event_handler() {
        let json = include_str!("../events/sample.json");
        let mut s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        // make it so bucket name != the expected request bucket name
        s3_event.records[0].s3.bucket.name = Some("test-other-request".to_string());

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        let opts = HandlerOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }

    #[tokio::test]
    async fn test_valid_event_handler() {
        // json contains bucket name == starts with stack name
        let json = include_str!("../events/sample.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        let opts = HandlerOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }
}
