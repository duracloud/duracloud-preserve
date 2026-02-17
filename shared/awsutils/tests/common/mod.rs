#![allow(dead_code)]

use apputils::Stack;
use aws_sdk_s3::Client;

use awsutils::{
    bucket::{delete, empty},
    config,
};

pub struct IntegrationTestContext {
    pub account_id: String,
    pub replication_role_arn: String,
    pub s3: Client,
    pub stack: Stack,
}

pub async fn integration_test_context() -> IntegrationTestContext {
    let stack_name = std::env::var("TEST_STACK").unwrap_or_else(|_| "int-test".to_string());
    let stack = Stack::new(&stack_name).expect("invalid stack name");

    let sdk_config = config::default_config().await;
    let s3 = Client::new(&sdk_config);

    let account_id = config::get_account_id(&sdk_config)
        .await
        .expect("failed to get account id");

    let replication_role_arn = config::get_role_arn(&sdk_config, &stack.replication_role_name())
        .await
        .expect("failed to get replication role arn");

    IntegrationTestContext {
        account_id,
        replication_role_arn,
        s3,
        stack,
    }
}

pub async fn cleanup_bucket(client: &Client, bucket: &str) {
    let _ = empty(client, bucket).await;
    let _ = delete(client, bucket).await;
}

pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
