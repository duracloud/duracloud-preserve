use awsutils::config::get_sftpgo_parameter;
use lambda_runtime::{Error, tracing};

mod event_handler;

use app::config as app_config;
use base::Stack;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let stack = env::var("STACK").expect("stack is required");
    let stack = Stack::new(&stack).expect("invalid stack name");

    let config = app_config::load(stack).await?;
    let ssm_client = &config.clients().ssm;

    let (sftpgo_host, sftpgo_username, sftpgo_password) = tokio::try_join!(
        get_sftpgo_parameter(ssm_client, "host"),
        get_sftpgo_parameter(ssm_client, "username"),
        get_sftpgo_parameter(ssm_client, "password"),
    )?;

    let handler_opts = event_handler::HandlerOptions {
        sftpgo_host,
        sftpgo_username,
        sftpgo_password,
    };

    lambda_runtime::run(lambda_runtime::service_fn(|event| {
        event_handler::function_handler(&config, &handler_opts, event)
    }))
    .await
}
