use app::{config::Config, perform::compute_checksums};
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use lambda_runtime::{Error, LambdaEvent, tracing};

pub(crate) async fn function_handler(
    config: &Config,
    perform_opts: &compute_checksums::PerformOptions,
    event: LambdaEvent<CloudWatchEvent>,
) -> Result<(), Error> {
    let payload = event.payload;
    tracing::info!("Schedule payload: {:?}", payload);

    if config.debug_handler {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    compute_checksums::perform(config, None, perform_opts).await?;

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
    async fn test_event_handler() {
        // This is very unexciting because a scheduled event doesn't have anything for us
        let event = LambdaEvent::new(CloudWatchEvent::default(), Context::default());
        let config = mock_config(true);
        let opts = compute_checksums::PerformOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }
}
