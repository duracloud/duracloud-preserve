use lambda_runtime::{run, service_fn, tracing, Error};

mod event_handler;
use event_handler::function_handler;

mod app;

use apputils::StackName;
use awsutils::bucket::RequestConfig;
use awsutils::file::test_client;
use std::env;

use aws_sdk_s3::primitives::SdkBody;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let test_client = test_client("".to_string(), SdkBody::from(""), None);

    let config = RequestConfig {
        debug_handler: false,
        s3_client: test_client,
        stack: StackName::new(&env::var("STACK").expect("Stack is required"))
            .expect("Invalid stack name"),
    };

    run(service_fn(|event| function_handler(&config, event))).await
}
