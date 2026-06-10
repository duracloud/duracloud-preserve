//! Integration tests for app bucket discovery.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test -p app --test bucket_discovery -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "int-test")
//!   - Run: mise run setup --stack <stack> --profile <profile>

use app::{bucket as app_bucket, config};
use aws_sdk_s3::types::TransitionStorageClass;
use awsutils::bucket::{self as aws_bucket, Bucket, Type};

async fn cleanup_buckets(client: &aws_sdk_s3::Client, bucket_names: &[String]) {
    for name in bucket_names {
        let _ = aws_bucket::delete(client, name).await;
    }
}

#[tokio::test]
#[ignore]
async fn test_get_stack_buckets_finds_tagged_stack_buckets() {
    let config = test_support::integration_test_config(config::load).await;
    let ts = test_support::unix_timestamp_secs();
    let standard_name = format!("{}-inttest-discovery-{}", config.stack().as_str(), ts);
    let replication_name = format!("{}-inttest-discovery-{}-repl", config.stack().as_str(), ts);
    let bucket_names = vec![standard_name.clone(), replication_name.clone()];

    let standard = Bucket::new(&standard_name, Type::Standard).unwrap();
    let replication = Bucket::new(&replication_name, Type::Replication).unwrap();
    let standard_creator = test_support::bucket_creator!(
        &standard,
        Some(TransitionStorageClass::GlacierIr),
        config.account_id(),
        config.s3(),
        config.replication_role_arn(),
        config.stack(),
    );
    let replication_creator = test_support::bucket_creator!(
        &replication,
        None,
        config.account_id(),
        config.s3(),
        config.replication_role_arn(),
        config.stack(),
    );

    standard_creator
        .create()
        .await
        .expect("bucket creation failed");
    replication_creator
        .create()
        .await
        .expect("bucket creation failed");

    let discovered = app_bucket::list_for_stack(config.s3(), config.stack(), None).await;
    cleanup_buckets(config.s3(), &bucket_names).await;
    let discovered = discovered.expect("bucket discovery failed");

    assert!(discovered.iter().any(|b| b.name() == standard_name));
    assert!(discovered.iter().any(|b| b.name() == replication_name));
}

#[tokio::test]
#[ignore]
async fn test_list_for_stack_by_type_filters_results() {
    let config = test_support::integration_test_config(config::load).await;
    let ts = test_support::unix_timestamp_secs();
    let standard_name = format!("{}-inttest-filter-{}", config.stack().as_str(), ts);
    let replication_name = format!("{}-inttest-filter-{}-repl", config.stack().as_str(), ts);
    let bucket_names = vec![standard_name.clone(), replication_name.clone()];

    let standard = Bucket::new(&standard_name, Type::Standard).unwrap();
    let replication = Bucket::new(&replication_name, Type::Replication).unwrap();
    let standard_creator = test_support::bucket_creator!(
        &standard,
        Some(TransitionStorageClass::GlacierIr),
        config.account_id(),
        config.s3(),
        config.replication_role_arn(),
        config.stack(),
    );
    let replication_creator = test_support::bucket_creator!(
        &replication,
        None,
        config.account_id(),
        config.s3(),
        config.replication_role_arn(),
        config.stack(),
    );

    standard_creator
        .create()
        .await
        .expect("bucket creation failed");
    replication_creator
        .create()
        .await
        .expect("bucket creation failed");

    let discovered =
        app_bucket::list_for_stack_by_type(config.s3(), config.stack(), &[Type::Standard]).await;
    cleanup_buckets(config.s3(), &bucket_names).await;
    let discovered = discovered.expect("typed bucket discovery failed");

    assert!(discovered.iter().any(|b| b.name() == standard_name));
    assert!(!discovered.iter().any(|b| b.name() == replication_name));
}
