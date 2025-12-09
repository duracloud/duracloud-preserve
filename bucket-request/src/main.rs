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

    let config = RequestConfig {
        debug_handler: false,
        s3_client: s3_client,
        stack: StackName::new(&env::var("STACK").expect("Stack is required"))
            .expect("Invalid stack name"),
    };

    run(service_fn(|event| function_handler(&config, event))).await
}
