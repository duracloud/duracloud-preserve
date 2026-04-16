#![allow(dead_code)]

use aws_sdk_s3::Client;
use aws_sdk_s3::types::TransitionStorageClass;
use base::Stack;

use awsutils::{
    bucket::{self, Bucket},
    bucket_creator::BucketCreator,
    config,
};

pub struct IntegrationTestContext {
    pub account_id: String,
    pub replication_role_arn: String,
    pub s3: Client,
    pub stack: Stack,
}

pub async fn integration_test_context() -> IntegrationTestContext {
    let stack = test_support::integration_test_stack();

    let sdk_config = config::load_defaults().await;
    let s3 = Client::new(&sdk_config);
    let sts = aws_sdk_sts::Client::new(&sdk_config);
    let iam = aws_sdk_iam::Client::new(&sdk_config);

    let account_id = config::get_account_id(&sts)
        .await
        .expect("failed to get account id");

    let replication_role_arn = config::get_role_arn(&iam, &stack.replication_role_name())
        .await
        .expect("failed to get replication role arn");

    IntegrationTestContext {
        account_id,
        replication_role_arn,
        s3,
        stack,
    }
}

pub fn bucket_creator<'a>(
    ctx: &'a IntegrationTestContext,
    bucket: &'a Bucket,
    storage_tier_override: Option<TransitionStorageClass>,
) -> BucketCreator<'a> {
    test_support::bucket_creator!(
        bucket,
        storage_tier_override,
        &ctx.account_id,
        &ctx.s3,
        &ctx.replication_role_arn,
        &ctx.stack,
    )
}

pub async fn cleanup_bucket(client: &Client, bucket: &str) {
    let _ = bucket::empty(client, bucket).await;
    let _ = bucket::delete(client, bucket).await;
}

pub fn timestamp() -> u64 {
    test_support::unix_timestamp_secs()
}
