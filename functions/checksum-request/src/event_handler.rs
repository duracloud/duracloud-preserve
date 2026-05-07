use app::{
    bucket,
    config::Config,
    perform::checksum_request::{self, PerformArgs},
};
use aws_lambda_events::event::s3::S3Event;
use awsutils::file::{self, File};
use base::stack::DateCtx;
use constants::CHECKSUM_REQUEST_PREFIX;
use lambda_runtime::{Error, LambdaEvent, tracing};

pub(crate) async fn function_handler(
    config: &Config,
    event: LambdaEvent<S3Event>,
) -> Result<(), Error> {
    let payload = event.payload;

    let record = payload.records.first().expect("payload should have record");
    let event_bucket = record.s3.bucket.name.as_ref().expect("bucket required");
    let object = record.s3.object.key.as_ref().expect("object requried");

    tracing::info!("Bucket: {:?}, Object: {:?}", event_bucket, object);

    if event_bucket != &config.stack().request_bucket()
        || !object.starts_with(&format!("{CHECKSUM_REQUEST_PREFIX}/"))
    {
        panic!(
            "Not the request bucket or path for this stack: {:?}",
            config.stack()
        );
    }

    if config.debug_handler() {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    let trigger = File::new(event_bucket, object);
    let bucket = bucket::name_from_file(&trigger).map_err(|e| Error::from(e.to_string()))?;

    let report = File::from(
        config
            .stack()
            .reports_manifests_path(bucket, DateCtx::Latest),
    );

    if !file::exists(config.s3(), &report).await {
        return Err(Error::from(format!(
            "Inventory report not found for bucket: {bucket}"
        )));
    }

    let args = PerformArgs::new(report);
    let inventory = checksum_request::perform(config, &args)
        .await
        .map_err(|e| Error::from(e.to_string()))?;

    tracing::info!("Checksum inventory uploaded to: {inventory}");

    if let Err(e) = file::delete(config.s3(), &trigger).await {
        tracing::warn!("Failed to delete trigger file {}: {e}", trigger.s3_url());
    }

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

        // make it so the object key is outside the checksums prefix
        s3_event.records[0].s3.object.key = Some("other/test-stack-private.txt".to_string());

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        function_handler(&config, event).await.unwrap();
    }

    #[tokio::test]
    async fn test_valid_event_handler() {
        // json contains bucket name == request bucket and key under checksums/
        let json = include_str!("../events/sample.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        function_handler(&config, event).await.unwrap();
    }
}
