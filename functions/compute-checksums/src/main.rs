use lambda_runtime::{Error, run, service_fn, tracing};

mod event_handler;
use event_handler::function_handler;

use app::perform::compute_checksums::PerformOptions;
use apputils::Stack;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let stack =
        Stack::new(&env::var("STACK").expect("Stack is required")).expect("Invalid stack name");
    let config = app::config::config(stack).await?;
    let perform_opts = PerformOptions::default();

    run(service_fn(|event| {
        function_handler(&config, &perform_opts, event)
    }))
    .await
}
