//! Integration tests for thin bucket wrappers.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test -p awsutils --test bucket -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "int-test")
//!   - Run: make setup s=<stack> p=<profile>

mod common;

use aws_sdk_s3::types::TransitionStorageClass;
use aws_smithy_types::body::SdkBody;
use awsutils::bucket::{self, Bucket, Type};
use common::{bucket_creator, integration_test_context, timestamp};

#[tokio::test]
#[ignore]
async fn test_bucket_from_name() {
    let ctx = integration_test_context().await;

    let ts = timestamp();
    let bucket_name = format!("{}-inttest-fromname-{}", ctx.stack.as_str(), ts);
    let bucket = Bucket::new(&bucket_name, Type::Standard).unwrap();
    let creator = bucket_creator(&ctx, &bucket, Some(TransitionStorageClass::GlacierIr));

    creator.create().await.expect("bucket creation failed");

    let result = bucket::from_name(&ctx.s3, &bucket_name)
        .await
        .expect("from_name failed");

    assert!(result.is_some(), "bucket should be found");
    let found_bucket = result.unwrap();
    assert_eq!(found_bucket.name(), bucket_name);
    assert_eq!(found_bucket.bucket_type(), &Type::Standard);

    // Test non-existent bucket returns None.
    let missing = bucket::from_name(&ctx.s3, "nonexistent-bucket-xyz")
        .await
        .expect("from_name should not error for missing bucket");
    assert!(missing.is_none(), "missing bucket should return None");

    bucket::delete(&ctx.s3, &bucket_name)
        .await
        .expect("cleanup failed");
}

#[tokio::test]
#[ignore]
async fn test_empty_bucket() {
    let ctx = integration_test_context().await;

    let ts = timestamp();
    let bucket_name = format!("{}-inttest-empty-{}", ctx.stack.as_str(), ts);
    let bucket = Bucket::new(&bucket_name, Type::Standard).unwrap();
    let creator = bucket_creator(&ctx, &bucket, Some(TransitionStorageClass::GlacierIr));

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    // Create some objects.
    ctx.s3
        .put_object()
        .bucket(&bucket_name)
        .key("test1.txt")
        .body(SdkBody::from("content1").into())
        .send()
        .await
        .expect("put object failed");

    ctx.s3
        .put_object()
        .bucket(&bucket_name)
        .key("test2.txt")
        .body(SdkBody::from("content2").into())
        .send()
        .await
        .expect("put object failed");

    // Overwrite to create versions.
    ctx.s3
        .put_object()
        .bucket(&bucket_name)
        .key("test1.txt")
        .body(SdkBody::from("content1-v2").into())
        .send()
        .await
        .expect("put object failed");

    bucket::empty(&ctx.s3, &bucket_name)
        .await
        .expect("empty_bucket failed");

    let list_result = ctx
        .s3
        .list_object_versions()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("list versions failed");

    assert!(
        list_result.versions().is_empty(),
        "bucket should have no versions"
    );
    assert!(
        list_result.delete_markers().is_empty(),
        "bucket should have no delete markers"
    );

    bucket::delete(&ctx.s3, &bucket_name)
        .await
        .expect("cleanup failed");

    assert!(
        !bucket::exists(&ctx.s3, &bucket_name).await,
        "bucket should not exist after deletion"
    );
}
