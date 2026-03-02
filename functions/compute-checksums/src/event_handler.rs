use app::{config::Config, perform::compute_checksums};
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use lambda_runtime::{Error, LambdaEvent, tracing};

pub(crate) async fn function_handler(
    config: &Config,
    event: LambdaEvent<CloudWatchEvent>,
) -> Result<(), Error> {
    let payload = event.payload;
    tracing::info!("Schedule payload: {:?}", payload);

    if config.debug_handler {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    compute_checksums::perform(config, None).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::config as app_config;
    use lambda_runtime::{Context, LambdaEvent};
    use test_support::TestClientBuilder;

    #[tokio::test]
    async fn test_event_handler() {
        // This is very unexciting because a scheduled event doesn't have anything for us
        let event = LambdaEvent::new(CloudWatchEvent::default(), Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        function_handler(&config, event).await.unwrap();
    }
}
