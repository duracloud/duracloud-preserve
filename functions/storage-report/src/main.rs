use lambda_runtime::{tracing, Error};

mod event_handler;

use app::config;
use apputils::Stack;
use std::env;
#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let stack =
        Stack::new(&env::var("STACK").expect("stack is required")).expect("invalid stack name");
    let config = config::load(stack).await?;

    lambda_runtime::run(lambda_runtime::service_fn(|event| {
        event_handler::function_handler(&config, event)
    }))
    .await
}
