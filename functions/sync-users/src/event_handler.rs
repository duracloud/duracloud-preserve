use app::{config::Config, perform::sync_users};
use aws_lambda_events::event::s3::S3Event;
use constants::{SYNC_USERS_FILE, SYNC_USERS_PREFIX};
use lambda_runtime::{Error, LambdaEvent, tracing};

#[derive(Debug, Clone, Default)]
pub(crate) struct HandlerOptions {
    pub sftpgo_host: String,
    pub sftpgo_username: String,
    pub sftpgo_password: String,
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

    let expected_key = format!("{SYNC_USERS_PREFIX}/{SYNC_USERS_FILE}");
    if bucket != &config.stack().managed_bucket() || object != &expected_key {
        panic!(
            "Not the managed bucket or trigger file for this stack: {:?}",
            config.stack()
        );
    }

    if config.debug_handler() {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    let args = sync_users::PerformArgs {
        username: None,
        sftpgo_host: handler_opts.sftpgo_host.clone(),
        sftpgo_username: handler_opts.sftpgo_username.clone(),
        sftpgo_password: handler_opts.sftpgo_password.clone(),
    };

    sync_users::perform(config.clients(), &args)
        .await
        .map_err(|e| Error::from(e.to_string()))?;

    config
        .s3()
        .delete_object()
        .bucket(bucket)
        .key(object)
        .send()
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
    #[should_panic(expected = "Not the managed bucket or trigger file for this stack")]
    async fn test_invalid_event_handler() {
        let json = include_str!("../events/sample.json");
        let mut s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        // make it so bucket name != the expected managed bucket name
        s3_event.records[0].s3.bucket.name = Some("test-other-managed".to_string());

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        let opts = HandlerOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }

    #[tokio::test]
    async fn test_valid_event_handler() {
        // json contains bucket name == stack managed bucket, key starts with sync-users prefix
        let json = include_str!("../events/sample.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        let opts = HandlerOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }
}
