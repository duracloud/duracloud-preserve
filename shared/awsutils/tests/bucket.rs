//! Integration tests for bucket discovery and operations.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test --test bucket -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "int-test")
//!   - Run: make setup s=<stack> p=<profile>

mod common;

use aws_sdk_s3::types::TransitionStorageClass;
use aws_smithy_types::body::SdkBody;
use awsutils::bucket::{
    Bucket, Type, delete, empty, exists, get_stack_buckets, get_stack_buckets_by_type,
};
use awsutils::bucket_creator::BucketCreator;
use awsutils::test_client::integration_test_config;
use common::timestamp;

#[tokio::test]
#[ignore]
async fn test_bucket_from_name() {
    let config = integration_test_config().await;

    let ts = timestamp();
    let bucket_name = format!("{}-inttest-fromname-{}", config.stack().as_str(), ts);
    let bucket = Bucket::new(&bucket_name, Type::Standard).unwrap();
    let creator = BucketCreator::new(&config, &bucket, Some(TransitionStorageClass::GlacierIr));

    creator.create().await.expect("bucket creation failed");

    let result = Bucket::from_name(config.s3(), &bucket_name)
        .await
        .expect("from_name failed");

    assert!(result.is_some(), "bucket should be found");
    let found_bucket = result.unwrap();
    assert_eq!(found_bucket.name(), bucket_name);
    assert_eq!(found_bucket.bucket_type(), &Type::Standard);

    // Test non-existent bucket returns None
    let missing = Bucket::from_name(config.s3(), "nonexistent-bucket-xyz")
        .await
        .expect("from_name should not error for missing bucket");
    assert!(missing.is_none(), "missing bucket should return None");

    delete(config.s3(), &bucket_name)
        .await
        .expect("cleanup failed");
}

#[tokio::test]
#[ignore]
async fn test_get_stack_buckets() {
    let config = integration_test_config().await;

    let ts = timestamp();
    let bucket_name = format!("{}-inttest-discovery-{}", config.stack().as_str(), ts);
    let bucket = Bucket::new(&bucket_name, Type::Standard).unwrap();
    let creator = BucketCreator::new(&config, &bucket, Some(TransitionStorageClass::GlacierIr));

    creator.create().await.expect("bucket creation failed");

    let buckets = get_stack_buckets(config.s3(), config.stack())
        .await
        .expect("get_stack_buckets failed");

    let found = buckets.iter().any(|b| b.name() == bucket_name);
    assert!(found, "test bucket not found in discovery");

    delete(config.s3(), &bucket_name)
        .await
        .expect("cleanup failed");
}

#[tokio::test]
#[ignore]
async fn test_get_stack_buckets_by_type() {
    let config = integration_test_config().await;

    let ts = timestamp();
    let std_name = format!("{}-inttest-type-std-{}", config.stack().as_str(), ts);
    let repl_name = format!("{}-inttest-type-repl-{}", config.stack().as_str(), ts);

    let std_bucket = Bucket::new(&std_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(&repl_name, Type::Replication).unwrap();

    let std_creator = BucketCreator::new(
        &config,
        &std_bucket,
        Some(TransitionStorageClass::GlacierIr),
    );
    let repl_creator = BucketCreator::new(&config, &repl_bucket, None);

    std_creator
        .create()
        .await
        .expect("std bucket creation failed");
    repl_creator
        .create()
        .await
        .expect("repl bucket creation failed");

    let std_buckets = get_stack_buckets_by_type(config.s3(), config.stack(), &[Type::Standard])
        .await
        .expect("get_stack_buckets_by_type failed");

    let found_std = std_buckets.iter().any(|b| b.name() == std_name);
    let found_repl_in_std = std_buckets.iter().any(|b| b.name() == repl_name);

    assert!(found_std, "standard bucket not found");
    assert!(
        !found_repl_in_std,
        "replication bucket should not be in standard filter"
    );

    delete(config.s3(), &std_name)
        .await
        .expect("cleanup failed");
    delete(config.s3(), &repl_name)
        .await
        .expect("cleanup failed");
}

#[tokio::test]
#[ignore]
async fn test_empty_bucket() {
    let config = integration_test_config().await;

    let ts = timestamp();
    let bucket_name = format!("{}-inttest-empty-{}", config.stack().as_str(), ts);
    let bucket = Bucket::new(&bucket_name, Type::Standard).unwrap();
    let creator = BucketCreator::new(&config, &bucket, Some(TransitionStorageClass::GlacierIr));

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    // Create some objects
    config
        .s3()
        .put_object()
        .bucket(&bucket_name)
        .key("test1.txt")
        .body(SdkBody::from("content1").into())
        .send()
        .await
        .expect("put object failed");

    config
        .s3()
        .put_object()
        .bucket(&bucket_name)
        .key("test2.txt")
        .body(SdkBody::from("content2").into())
        .send()
        .await
        .expect("put object failed");

    // Overwrite to create versions
    config
        .s3()
        .put_object()
        .bucket(&bucket_name)
        .key("test1.txt")
        .body(SdkBody::from("content1-v2").into())
        .send()
        .await
        .expect("put object failed");

    empty(config.s3(), &bucket_name)
        .await
        .expect("empty_bucket failed");

    let list_result = config
        .s3()
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

    delete(config.s3(), &bucket_name)
        .await
        .expect("cleanup failed");

    assert!(
        !exists(config.s3(), &bucket_name).await,
        "bucket should not exist after deletion"
    );
}
