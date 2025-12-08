use aws_lambda_events::event::s3::S3Event;
use lambda_runtime::{tracing, Error, LambdaEvent};

use crate::app::perform;

use awsutils::bucket::RequestConfig;
use awsutils::file::File;

pub(crate) async fn function_handler(
    config: &RequestConfig,
    event: LambdaEvent<S3Event>,
) -> Result<(), Error> {
    let payload = event.payload;

    // Moderate by only considering the first record (we've never seen more)
    let record = payload.records.first().expect("Payload should have record");
    let bucket = record.s3.bucket.name.as_ref().expect("Bucket required");
    let object = record.s3.object.key.as_ref().expect("Object requried");

    tracing::info!("Bucket: {:?}, Object: {:?}", bucket, object);

    if !(bucket == &config.stack.request_bucket()) {
        panic!("Not the request bucket for this stack: {:?}", config.stack);
    }

    if config.debug_handler {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    perform(config, &File::new(bucket.to_owned(), object.to_owned())).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use apputils::StackName;
    use aws_sdk_s3::primitives::SdkBody;
    use awsutils::file::test_client;
    use lambda_runtime::{Context, LambdaEvent};

    #[tokio::test]
    async fn test_valid_event_handler() {
        // json contains bucket name == starts with stack name
        let json = include_str!("../events/stack.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("Failed to parse json");

        let bucket = s3_event.records[0].s3.bucket.name.as_ref().unwrap();
        let object = s3_event.records[0].s3.object.key.as_ref().unwrap();

        let test_client = test_client(
            File::new(bucket.to_owned(), object.to_owned()).http_url(),
            SdkBody::empty(),
        );

        let event = LambdaEvent::new(s3_event, Context::default());
        let config = RequestConfig {
            debug_handler: true,
            s3_client: test_client,
            stack: StackName::new("test-stack").unwrap(),
        };
        let response = function_handler(&config, event).await.unwrap();
        assert_eq!((), response);
    }

    #[tokio::test]
    #[should_panic(expected = "Not the request bucket for this stack")]
    async fn test_invalid_event_handler() {
        let json = include_str!("../events/stack.json");
        let mut s3_event: S3Event = serde_json::from_str(json).expect("Failed to parse json");

        // make it so bucket name != the expected request bucket name
        s3_event.records[0].s3.bucket.name = Some("test-other-bucket-request".to_string());
        let bucket = s3_event.records[0].s3.bucket.name.as_ref().unwrap();
        let object = s3_event.records[0].s3.object.key.as_ref().unwrap();

        let test_client = test_client(
            File::new(bucket.to_owned(), object.to_owned()).http_url(),
            SdkBody::empty(),
        );

        let event = LambdaEvent::new(s3_event, Context::default());
        let config = RequestConfig {
            debug_handler: true,
            s3_client: test_client,
            stack: StackName::new("test-stack").unwrap(),
        };
        function_handler(&config, event).await.unwrap();
    }
}
