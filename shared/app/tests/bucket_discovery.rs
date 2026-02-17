//! Integration tests for app bucket discovery.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test -p app --test bucket_discovery -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "int-test")
//!   - Run: make setup s=<stack> p=<profile>

use app::bucket::{get_stack_buckets, get_stack_buckets_by_type};
use app::config::Config;
use apputils::Stack;
use aws_sdk_s3::types::TransitionStorageClass;
use awsutils::bucket::{Bucket, Type, delete};
use awsutils::bucket_creator::{BucketCreator, BucketCreatorParams};

fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn integration_test_config() -> Config {
    let stack_name = std::env::var("TEST_STACK").unwrap_or_else(|_| "int-test".to_string());
    let stack = Stack::new(&stack_name).expect("invalid stack name");
    app::config::config(stack)
        .await
        .expect("failed to build integration test config")
}

fn bucket_creator<'a>(
    config: &'a Config,
    bucket: &'a Bucket,
    storage_tier_override: Option<TransitionStorageClass>,
) -> BucketCreator<'a> {
    BucketCreator::new(
        BucketCreatorParams {
            account_id: config.account_id(),
            client: config.s3(),
            replication_role_arn: config.replication_role_arn(),
            stack: config.stack(),
        },
        bucket,
        storage_tier_override,
    )
}

async fn cleanup_buckets(client: &aws_sdk_s3::Client, bucket_names: &[String]) {
    for name in bucket_names {
        let _ = delete(client, name).await;
    }
}

#[tokio::test]
#[ignore]
async fn test_get_stack_buckets_finds_tagged_stack_buckets() {
    let config = integration_test_config().await;
    let ts = timestamp();
    let standard_name = format!("{}-inttest-discovery-{}", config.stack().as_str(), ts);
    let replication_name = format!("{}-inttest-discovery-{}-repl", config.stack().as_str(), ts);
    let bucket_names = vec![standard_name.clone(), replication_name.clone()];

    let standard = Bucket::new(&standard_name, Type::Standard).unwrap();
    let replication = Bucket::new(&replication_name, Type::Replication).unwrap();
    let standard_creator =
        bucket_creator(&config, &standard, Some(TransitionStorageClass::GlacierIr));
    let replication_creator = bucket_creator(&config, &replication, None);

    standard_creator
        .create()
        .await
        .expect("bucket creation failed");
    replication_creator
        .create()
        .await
        .expect("bucket creation failed");

    let discovered = get_stack_buckets(config.s3(), config.stack()).await;
    cleanup_buckets(config.s3(), &bucket_names).await;
    let discovered = discovered.expect("bucket discovery failed");

    assert!(discovered.iter().any(|b| b.name() == standard_name));
    assert!(discovered.iter().any(|b| b.name() == replication_name));
}

#[tokio::test]
#[ignore]
async fn test_get_stack_buckets_by_type_filters_results() {
    let config = integration_test_config().await;
    let ts = timestamp();
    let standard_name = format!("{}-inttest-filter-{}", config.stack().as_str(), ts);
    let replication_name = format!("{}-inttest-filter-{}-repl", config.stack().as_str(), ts);
    let bucket_names = vec![standard_name.clone(), replication_name.clone()];

    let standard = Bucket::new(&standard_name, Type::Standard).unwrap();
    let replication = Bucket::new(&replication_name, Type::Replication).unwrap();
    let standard_creator =
        bucket_creator(&config, &standard, Some(TransitionStorageClass::GlacierIr));
    let replication_creator = bucket_creator(&config, &replication, None);

    standard_creator
        .create()
        .await
        .expect("bucket creation failed");
    replication_creator
        .create()
        .await
        .expect("bucket creation failed");

    let discovered =
        get_stack_buckets_by_type(config.s3(), config.stack(), &[Type::Standard]).await;
    cleanup_buckets(config.s3(), &bucket_names).await;
    let discovered = discovered.expect("typed bucket discovery failed");

    assert!(discovered.iter().any(|b| b.name() == standard_name));
    assert!(!discovered.iter().any(|b| b.name() == replication_name));
}
