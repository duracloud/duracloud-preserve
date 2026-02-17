//! Integration tests for bucket creation and configuration.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test -p awsutils --test bucket_creator -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "int-test")
//!   - Run: make setup s=<stack> p=<profile>

mod common;

use aws_sdk_s3::types::{BucketVersioningStatus, TransitionStorageClass};
use awsutils::bucket::{Bucket, Type, exists};
use awsutils::bucket_creator::{BucketCreator, BucketCreatorParams};
use common::{cleanup_bucket, integration_test_context, timestamp};

fn bucket_creator<'a>(
    ctx: &'a common::IntegrationTestContext,
    bucket: &'a Bucket,
    storage_tier_override: Option<TransitionStorageClass>,
) -> BucketCreator<'a> {
    BucketCreator::new(
        BucketCreatorParams {
            account_id: &ctx.account_id,
            client: &ctx.s3,
            replication_role_arn: &ctx.replication_role_arn,
            stack: &ctx.stack,
        },
        bucket,
        storage_tier_override,
    )
}

#[tokio::test]
#[ignore]
async fn test_create_standard_bucket() {
    let ctx = integration_test_context().await;
    let bucket_name = format!("{}-inttest-std-{}", ctx.stack.as_str(), timestamp());

    let bucket = Bucket::new(&bucket_name, Type::Standard).unwrap();
    let creator = bucket_creator(&ctx, &bucket, Some(TransitionStorageClass::GlacierIr));

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    assert!(exists(&ctx.s3, &bucket_name).await);

    let versioning = ctx
        .s3
        .get_bucket_versioning()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("failed to fetch versioning");

    assert_eq!(versioning.status(), Some(&BucketVersioningStatus::Enabled));

    cleanup_bucket(&ctx.s3, &bucket_name).await;
}

#[tokio::test]
#[ignore]
async fn test_rollback_deletes_bucket() {
    let ctx = integration_test_context().await;
    let bucket_name = format!("{}-inttest-rollback-{}", ctx.stack.as_str(), timestamp());
    let bucket = Bucket::new(&bucket_name, Type::Standard).unwrap();

    let creator = bucket_creator(&ctx, &bucket, Some(TransitionStorageClass::GlacierIr));
    creator.create().await.expect("bucket creation failed");

    assert!(
        exists(&ctx.s3, &bucket_name).await,
        "bucket should exist after creation"
    );

    creator.rollback().await.expect("rollback failed");

    assert!(
        !exists(&ctx.s3, &bucket_name).await,
        "bucket should not exist after rollback"
    );
}
