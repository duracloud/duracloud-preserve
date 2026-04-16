use lambda_runtime::{Error, tracing};

mod event_handler;

use app::config as app_config;
use awsutils::{bucket_creator, config};
use base::Stack;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let stack = env::var("STACK").expect("stack is required");
    let stack = Stack::new(&stack).expect("invalid stack name");
    let standard_storage_tier = env::var("STORAGE_TIER").expect("storage tier is required");

    let config = app_config::load(stack).await?;

    let standard_storage_tier = match config::parse_storage_class(&standard_storage_tier) {
        Some(tier) => tier,
        None => bucket_creator::STORAGE_CLASS_STANDARD_DEFAULT,
    };

    let handler_opts = event_handler::HandlerOptions {
        standard_storage_tier,
    };

    lambda_runtime::run(lambda_runtime::service_fn(|event| {
        event_handler::function_handler(&config, &handler_opts, event)
    }))
    .await
}
