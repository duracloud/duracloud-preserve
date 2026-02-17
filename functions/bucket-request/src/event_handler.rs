use app::{
    config::Config,
    perform::bucket_request::{self, PerformOptions},
};
use aws_lambda_events::event::s3::S3Event;
use awsutils::file::File;
use lambda_runtime::{Error, LambdaEvent, tracing};

pub(crate) async fn function_handler(
    config: &Config,
    perform_opts: &PerformOptions,
    event: LambdaEvent<S3Event>,
) -> Result<(), Error> {
    let payload = event.payload;

    let record = payload.records.first().expect("Payload should have record");
    let bucket = record.s3.bucket.name.as_ref().expect("Bucket required");
    let object = record.s3.object.key.as_ref().expect("Object requried");

    tracing::info!("Bucket: {:?}, Object: {:?}", bucket, object);

    if bucket != &config.stack().request_bucket() {
        panic!(
            "Not the request bucket for this stack: {:?}",
            config.stack()
        );
    }

    if config.debug_handler {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    bucket_request::perform(config, &File::new(bucket, object), perform_opts)
        .await
        .map_err(|e| Error::from(e.to_string()))?;

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
        // json contains bucket name == starts with stack name
        let json = include_str!("../events/sample.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("Failed to parse json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let config = mock_config(true);
        let opts = PerformOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "Not the request bucket for this stack")]
    async fn test_invalid_event_handler() {
        let json = include_str!("../events/sample.json");
        let mut s3_event: S3Event = serde_json::from_str(json).expect("Failed to parse json");

        // make it so bucket name != the expected request bucket name
        s3_event.records[0].s3.bucket.name = Some("test-other-bucket-request".to_string());

        let event = LambdaEvent::new(s3_event, Context::default());
        let config = mock_config(true);
        let opts = PerformOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }
}
