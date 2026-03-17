use lambda_runtime::{Error, run, service_fn, tracing};

mod event_handler;

use apputils::Stack;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let stack =
        Stack::new(&env::var("STACK").expect("stack is required")).expect("invalid stack name");
    let config = app::config::load(stack).await?;

    run(service_fn(|event| {
        event_handler::function_handler(&config, event)
    }))
    .await
}
