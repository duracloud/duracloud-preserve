use lambda_runtime::{run, service_fn, tracing, Error};

mod event_handler;
use event_handler::function_handler;

mod app;

use apputils::StackName;
use awsutils::bucket::RequestConfig;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let client_config = awsutils::config::default_config().await;
    let s3_client = aws_sdk_s3::Client::new(&client_config);

    let stack =
        StackName::new(&env::var("STACK").expect("Stack is required")).expect("Invalid stack name");

    let account_id = awsutils::config::get_account_id(&client_config)
        .await
        .expect("Failed to get AWS account ID");

    let replication_role_arn = awsutils::config::get_replication_role_arn(&client_config, &stack)
        .await
        .expect("Failed to get replication role ARN - run scripts/create-replication-role.sh");

    let config = RequestConfig {
        account_id,
        debug_handler: false,
        replication_role_arn,
        s3_client,
        stack,
    };

    run(service_fn(|event| function_handler(&config, event))).await
}
