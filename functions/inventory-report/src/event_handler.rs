use app::{config::Config, perform::inventory_report};
use aws_lambda_events::event::s3::S3Event;
use awsutils::file::File;
use lambda_runtime::{Error, LambdaEvent, tracing};

pub(crate) async fn function_handler(
    config: &Config,
    perform_opts: &inventory_report::PerformOptions,
    event: LambdaEvent<S3Event>,
) -> Result<(), Error> {
    let payload = event.payload;

    let record = payload.records.first().expect("Payload should have record");
    let bucket = record.s3.bucket.name.as_ref().expect("Bucket required");
    let object = record.s3.object.key.as_ref().expect("Object requried");

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

    if config.debug_handler {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    let stats = inventory_report::perform(config, &File::new(bucket, object), perform_opts).await?;

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
    use app::config::{Clients, Roles};
    use apputils::Stack;
    use lambda_runtime::{Context, LambdaEvent};
    use test_support::TestClientBuilder;

    fn mock_config(debug_handler: bool) -> Config {
        let client = TestClientBuilder::new().ok().build();
        let stack = Stack::new("test-stack").unwrap();
        let sdk_config = aws_config::SdkConfig::builder()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new("us-east-1"))
            .build();

        let roles = Roles {
            batch: "arn:aws:iam::123456789:role/test-batch-role".to_string(),
            replication: "arn:aws:iam::123456789:role/test-replication-role".to_string(),
        };
        let clients = Clients::with_s3(&sdk_config, client);

        Config::new_with_clients(
            sdk_config,
            "123456789".to_string(),
            roles,
            stack,
            debug_handler,
            clients,
        )
    }

    #[tokio::test]
    async fn test_valid_event_handler() {
        // json contains object key == manifest.json
        let json = include_str!("../events/sample.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("Failed to parse json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let config = mock_config(true);
        let opts = inventory_report::PerformOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "Not an inventory manifest file")]
    async fn test_invalid_event_handler() {
        let json = include_str!("../events/sample.json");
        let mut s3_event: S3Event = serde_json::from_str(json).expect("Failed to parse json");

        // make it so object key != the expected manifest.json
        s3_event.records[0].s3.object.key = Some("something-else.json".to_string());

        let event = LambdaEvent::new(s3_event, Context::default());
        let config = mock_config(true);
        let opts = inventory_report::PerformOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }
}
