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
    use apputils::Stack;
    use awsutils::test_client::{test_config_with_client_and_stack, TestClientBuilder};
    use lambda_runtime::{Context, LambdaEvent};

    fn test_config(debug_handler: bool) -> Config {
        let client = TestClientBuilder::new().ok().build();
        let stack = Stack::new("test-stack").unwrap();
        let mut config = test_config_with_client_and_stack(client, stack);
        config.debug_handler = debug_handler;
        config
    }

    #[tokio::test]
    async fn test_event_handler() {
        let event = LambdaEvent::new(CloudWatchEvent::default(), Context::default());
        let config = test_config(true);
        function_handler(&config, event).await.unwrap();
    }
}
