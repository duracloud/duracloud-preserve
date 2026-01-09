use lambda_runtime::{run, service_fn, tracing, Error};

mod event_handler;
use event_handler::function_handler;

mod app;

use apputils::StackName;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let stack =
        StackName::new(&env::var("STACK").expect("Stack is required")).expect("Invalid stack name");
    let request_config = awsutils::config::bucket_config(stack).await;

    run(service_fn(|event| function_handler(&request_config, event))).await
}
