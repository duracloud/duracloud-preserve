use aws_lambda_events::event::s3::S3Event;
use lambda_runtime::{tracing, Error, LambdaEvent};

use awsutils::bucket::BucketRequestConfig;
use awsutils::s3::File;

pub(crate) async fn function_handler(
    config: &BucketRequestConfig,
    event: LambdaEvent<S3Event>,
) -> Result<(), Error> {
    let payload = event.payload;

    let mut files: Vec<File> = Vec::new();

    for record in payload.records {
        let bucket = record.s3.bucket.name.expect("Bucket name required");
        let object = record.s3.object.key.expect("Object key requried");
        tracing::info!("Bucket: {:?}, Object: {:?}", bucket, object);
        if !bucket.starts_with(&config.stack) {
            panic!("Bucket is not eligible for this stack: {:?}", config.stack);
        }

        files.push(File::new(bucket, object));
    }

    if config.debug {
        tracing::info!("Debug mode enabled, skipping run function.");
        return Ok(());
    }

    // TODO: app::run(config, files);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lambda_runtime::{Context, LambdaEvent};

    #[tokio::test]
    async fn test_valid_event_handler() {
        let json = include_str!("../events/valid_stack.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("Failed to parse valid.json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let config = BucketRequestConfig {
            debug: true,
            stack: "app-test".to_string(),
        };
        let response = function_handler(&config, event).await.unwrap();
        assert_eq!((), response);
    }

    #[tokio::test]
    #[should_panic(expected = "Bucket is not eligible for this stack")]
    async fn test_invalid_event_handler() {
        let json = include_str!("../events/invalid_stack.json");
        let s3_event: S3Event = serde_json::from_str(json).expect("Failed to parse invalid.json");

        let event = LambdaEvent::new(s3_event, Context::default());
        let config = BucketRequestConfig {
            debug: true,
            stack: "app-test".to_string(),
        };
        function_handler(&config, event).await.unwrap();
    }
}
