use lambda_runtime::{Error, run, service_fn, tracing};

mod event_handler;
use event_handler::function_handler;

use app::perform::bucket_request;
use apputils::Stack;
use awsutils::{bucket_creator, config::parse_storage_class};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let stack = env::var("STACK").expect("stack is required");
    let stack = Stack::new(&stack).expect("invalid stack name");
    let standard_storage_tier = env::var("STORAGE_TIER").expect("storage tier is required");

    let config = app::config::config(stack).await?;

    let standard_storage_tier = match parse_storage_class(&standard_storage_tier) {
        Some(tier) => tier,
        None => bucket_creator::STORAGE_CLASS_STANDARD_DEFAULT,
    };

    let perform_opts = bucket_request::PerformOptions {
        standard_storage_tier,
    };

    run(service_fn(|event| {
        function_handler(&config, &perform_opts, event)
    }))
    .await
}
