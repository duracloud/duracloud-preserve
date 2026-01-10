//! Integration tests for bucket discovery and operations.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test --test bucket -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "inttest")
//!   - Run: make setup s=<stack> p=<profile>

mod common;

use aws_smithy_types::body::SdkBody;
use awsutils::bucket::{
    Bucket, Name, Type, bucket_exists, delete_bucket, empty_bucket, get_stack_buckets,
    get_stack_buckets_by_type,
};
use awsutils::bucket_creator::BucketCreator;
use awsutils::config::test_config;
use common::timestamp;

#[tokio::test]
#[ignore]
async fn test_get_stack_buckets() {
    let config = test_config().await;

    let ts = timestamp();
    let bucket_name = format!("{}-inttest-discovery-{}", config.stack.as_str(), ts);
    let bucket = Bucket(Name::new(&bucket_name).unwrap(), Type::Standard);
    let creator = BucketCreator::new(&config, &bucket);

    creator.create().await.expect("bucket creation failed");

    let buckets = get_stack_buckets(&config.s3_client, &config.stack)
        .await
        .expect("get_stack_buckets failed");

    let found = buckets.iter().any(|b| b.0.as_str() == bucket_name);
    assert!(found, "test bucket not found in discovery");

    delete_bucket(&config.s3_client, &bucket_name)
        .await
        .expect("cleanup failed");
}

#[tokio::test]
#[ignore]
async fn test_get_stack_buckets_by_type() {
    let config = test_config().await;

    let ts = timestamp();
    let std_name = format!("{}-inttest-type-std-{}", config.stack.as_str(), ts);
    let repl_name = format!("{}-inttest-type-repl-{}", config.stack.as_str(), ts);

    let std_bucket = Bucket(Name::new(&std_name).unwrap(), Type::Standard);
    let repl_bucket = Bucket(Name::new(&repl_name).unwrap(), Type::Replication);

    let std_creator = BucketCreator::new(&config, &std_bucket);
    let repl_creator = BucketCreator::new(&config, &repl_bucket);

    std_creator
        .create()
        .await
        .expect("std bucket creation failed");
    repl_creator
        .create()
        .await
        .expect("repl bucket creation failed");

    let std_buckets =
        get_stack_buckets_by_type(&config.s3_client, &config.stack, &[Type::Standard])
            .await
            .expect("get_stack_buckets_by_type failed");

    let found_std = std_buckets.iter().any(|b| b.0.as_str() == std_name);
    let found_repl_in_std = std_buckets.iter().any(|b| b.0.as_str() == repl_name);

    assert!(found_std, "standard bucket not found");
    assert!(
        !found_repl_in_std,
        "replication bucket should not be in standard filter"
    );

    delete_bucket(&config.s3_client, &std_name)
        .await
        .expect("cleanup failed");
    delete_bucket(&config.s3_client, &repl_name)
        .await
        .expect("cleanup failed");
}

#[tokio::test]
#[ignore]
async fn test_empty_bucket() {
    let config = test_config().await;

    let ts = timestamp();
    let bucket_name = format!("{}-inttest-empty-{}", config.stack.as_str(), ts);
    let bucket = Bucket(Name::new(&bucket_name).unwrap(), Type::Standard);
    let creator = BucketCreator::new(&config, &bucket);

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    // Create some objects
    config
        .s3_client
        .put_object()
        .bucket(&bucket_name)
        .key("test1.txt")
        .body(SdkBody::from("content1").into())
        .send()
        .await
        .expect("put object failed");

    config
        .s3_client
        .put_object()
        .bucket(&bucket_name)
        .key("test2.txt")
        .body(SdkBody::from("content2").into())
        .send()
        .await
        .expect("put object failed");

    // Overwrite to create versions
    config
        .s3_client
        .put_object()
        .bucket(&bucket_name)
        .key("test1.txt")
        .body(SdkBody::from("content1-v2").into())
        .send()
        .await
        .expect("put object failed");

    empty_bucket(&config.s3_client, &bucket_name)
        .await
        .expect("empty_bucket failed");

    let list_result = config
        .s3_client
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

    delete_bucket(&config.s3_client, &bucket_name)
        .await
        .expect("cleanup failed");

    assert!(
        !bucket_exists(&config.s3_client, &bucket_name).await,
        "bucket should not exist after deletion"
    );
}
