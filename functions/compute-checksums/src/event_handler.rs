use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use awsutils::config::Config;
use lambda_runtime::{tracing, Error, LambdaEvent};

pub(crate) async fn function_handler(
    config: &Config,
    event: LambdaEvent<CloudWatchEvent>,
) -> Result<(), Error> {
    let payload = event.payload;
    tracing::info!("Payload: {:?}", payload);

    if config.debug_handler {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use awsutils::test_client::MockConfigBuilder;
    use lambda_runtime::{Context, LambdaEvent};

    #[tokio::test]
    async fn test_event_handler() {
        let event = LambdaEvent::new(CloudWatchEvent::default(), Context::default());
        let config = MockConfigBuilder::new().debug_handler(true).build();
        function_handler(&config, event).await.unwrap();
    }
}
