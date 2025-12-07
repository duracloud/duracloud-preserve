use lambda_runtime::{run, service_fn, tracing, Error};

mod event_handler;
use event_handler::function_handler;

use awsutils::bucket::BucketRequestConfig;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let config = BucketRequestConfig {
        debug: false,
        stack: env::var("STACK").expect("Stack must be set"),
    };

    run(service_fn(|event| function_handler(&config, event))).await
}
