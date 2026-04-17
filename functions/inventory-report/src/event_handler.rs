use app::{config::Config, perform::inventory_report};
use aws_lambda_events::event::s3::S3Event;
use awsutils::file::File;
use lambda_runtime::{Error, LambdaEvent, tracing};

pub(crate) async fn function_handler(
    config: &Config,
    event: LambdaEvent<S3Event>,
) -> Result<(), Error> {
    let payload = event.payload;

    let record = payload.records.first().expect("payload should have record");
    let bucket = record.s3.bucket.name.as_ref().expect("bucket required");
    let object = record.s3.object.key.as_ref().expect("object requried");

    tracing::info!("Bucket: {:?}, Object: {:?}", bucket, object);

    if bucket != &config.stack().managed_bucket() {
        panic!(
            "Not the managed bucket for this stack: {:?}",
            config.stack()
        );
    }

    if !object.ends_with("manifest.json") {
        panic!("Not an inventory manifest file: {:?}", object);
    }

    if config.debug_handler() {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    let args = inventory_report::PerformArgs::new(File::new(bucket, object));
    let stats = inventory_report::perform(config, &args).await?;

    tracing::info!(
        "Processed {} files, {} bytes total",
        stats.total_files,
        stats.total_size
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::config as app_config;
    use lambda_runtime::{Context, LambdaEvent};
    use test_support::TestClientBuilder;

    #[tokio::test]
    #[should_panic(expected = "Not an inventory manifest file")]
    async fn test_invalid_event_handler() {
        let json = include_str!("../events/sample.json");
        let mut s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        // make it so object key != the expected manifest.json
        s3_event.records[0].s3.object.key = Some("something-else.json".to_string());

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        function_handler(&config, event).await.unwrap();
    }

    #[tokio::test]
    async fn test_valid_event_handler() {
        // json contains object key == manifest.json
        let json = include_str!("../events/sample.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("failed to parse json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        function_handler(&config, event).await.unwrap();
    }
}
