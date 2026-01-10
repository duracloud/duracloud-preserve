//! Integration tests for bucket creation and configuration.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test --test integration_test -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "inttest")
//!   - Run: make setup s=<stack> p=<profile>

mod common;

use aws_sdk_s3::types::BucketVersioningStatus;
use awsutils::bucket::{
    Bucket, Name, RequestConfig, Type, bucket_exists, delete_bucket, empty_bucket,
};
use awsutils::bucket_creator::BucketCreator;
use awsutils::config::test_config;
use common::timestamp;

// --- Verification Helpers ---

async fn verify_versioning_enabled(config: &RequestConfig, bucket: &str) {
    let result = config
        .s3_client
        .get_bucket_versioning()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get versioning");

    assert_eq!(
        result.status(),
        Some(&BucketVersioningStatus::Enabled),
        "versioning not enabled on {}",
        bucket
    );
}

async fn verify_lifecycle_policy(config: &RequestConfig, bucket: &str, expected_class: &str) {
    let result = config
        .s3_client
        .get_bucket_lifecycle_configuration()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get lifecycle");

    let rules = result.rules();
    assert!(!rules.is_empty(), "no lifecycle rules on {}", bucket);

    let has_expected_transition = rules.iter().any(|rule| {
        rule.id()
            .map(|id| id.contains(expected_class))
            .unwrap_or(false)
    });
    assert!(
        has_expected_transition,
        "expected {} transition rule on {}",
        expected_class, bucket
    );
}

async fn verify_notifications_enabled(config: &RequestConfig, bucket: &str) {
    let result = config
        .s3_client
        .get_bucket_notification_configuration()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get notifications");

    assert!(
        result.event_bridge_configuration().is_some(),
        "EventBridge not configured on {}",
        bucket
    );
}

async fn verify_inventory_configured(config: &RequestConfig, bucket: &str) {
    let result = config
        .s3_client
        .get_bucket_inventory_configuration()
        .bucket(bucket)
        .id("inventory")
        .send()
        .await
        .expect("failed to get inventory");

    let inv = result
        .inventory_configuration()
        .expect("no inventory config");

    assert!(inv.is_enabled(), "inventory not enabled on {}", bucket);
}

async fn verify_logging_enabled(config: &RequestConfig, bucket: &str) {
    let result = config
        .s3_client
        .get_bucket_logging()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get logging");

    let logging = result
        .logging_enabled()
        .expect(&format!("logging not enabled on {}", bucket));
    assert!(
        logging.target_prefix().contains("audit/"),
        "unexpected logging prefix on {}",
        bucket
    );
}

async fn verify_replication_configured(config: &RequestConfig, src: &str, dest: &str) {
    let result = config
        .s3_client
        .get_bucket_replication()
        .bucket(src)
        .send()
        .await
        .expect("failed to get replication");

    let repl_config = result
        .replication_configuration()
        .expect("no replication config");
    let rules = repl_config.rules();
    assert!(!rules.is_empty(), "no replication rules on {}", src);

    let dest_arn = format!("arn:aws:s3:::{}", dest);
    let has_dest = rules.iter().any(|rule| {
        rule.destination()
            .map(|d| d.bucket() == &dest_arn)
            .unwrap_or(false)
    });
    assert!(
        has_dest,
        "replication destination {} not found on {}",
        dest, src
    );
}

async fn verify_public_access_block_disabled(config: &RequestConfig, bucket: &str) {
    let result = config
        .s3_client
        .get_public_access_block()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get public access block");

    let pab = result
        .public_access_block_configuration()
        .expect("no config");
    assert_eq!(
        pab.block_public_acls(),
        Some(false),
        "block_public_acls should be false on {}",
        bucket
    );
    assert_eq!(
        pab.block_public_policy(),
        Some(false),
        "block_public_policy should be false on {}",
        bucket
    );
}

async fn verify_public_read_policy(config: &RequestConfig, bucket: &str) {
    let result = config
        .s3_client
        .get_bucket_policy()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get bucket policy");

    let policy = result.policy().expect("no policy");
    assert!(
        policy.contains("AllowPublicRead"),
        "AllowPublicRead not in policy on {}",
        bucket
    );
    assert!(
        policy.contains("s3:GetObject"),
        "s3:GetObject not in policy on {}",
        bucket
    );
}

async fn verify_no_bucket_policy(config: &RequestConfig, bucket: &str) {
    let result = config
        .s3_client
        .get_bucket_policy()
        .bucket(bucket)
        .send()
        .await;

    assert!(
        result.is_err(),
        "expected no bucket policy on {}, but found one",
        bucket
    );
}

async fn cleanup_bucket(config: &RequestConfig, bucket: &str) {
    let _ = empty_bucket(&config.s3_client, bucket).await;
    let _ = delete_bucket(&config.s3_client, bucket).await;
}

// --- Test Cases ---

#[tokio::test]
#[ignore]
async fn test_create_standard_bucket() {
    let config = test_config().await;
    let bucket_name = format!("{}-inttest-std-{}", config.stack.as_str(), timestamp());

    let bucket = Bucket(Name::new(&bucket_name).unwrap(), Type::Standard);
    let creator = BucketCreator::new(&config, &bucket);

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    verify_versioning_enabled(&config, &bucket_name).await;
    verify_lifecycle_policy(&config, &bucket_name, "GLACIER_IR").await;
    verify_notifications_enabled(&config, &bucket_name).await;
    verify_inventory_configured(&config, &bucket_name).await;
    verify_logging_enabled(&config, &bucket_name).await;
    verify_no_bucket_policy(&config, &bucket_name).await;

    cleanup_bucket(&config, &bucket_name).await;
}

#[tokio::test]
#[ignore]
async fn test_create_public_bucket() {
    let config = test_config().await;
    let bucket_name = format!("{}-inttest-pub-{}", config.stack.as_str(), timestamp());

    let bucket = Bucket(Name::new(&bucket_name).unwrap(), Type::Public);
    let creator = BucketCreator::new(&config, &bucket);

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    verify_versioning_enabled(&config, &bucket_name).await;
    verify_lifecycle_policy(&config, &bucket_name, "INTELLIGENT_TIERING").await;
    verify_notifications_enabled(&config, &bucket_name).await;
    verify_inventory_configured(&config, &bucket_name).await;
    verify_logging_enabled(&config, &bucket_name).await;
    verify_public_access_block_disabled(&config, &bucket_name).await;
    verify_public_read_policy(&config, &bucket_name).await;

    cleanup_bucket(&config, &bucket_name).await;
}

#[tokio::test]
#[ignore]
async fn test_create_replication_bucket() {
    let config = test_config().await;
    let bucket_name = format!("{}-inttest-repl-{}", config.stack.as_str(), timestamp());

    let bucket = Bucket(Name::new(&bucket_name).unwrap(), Type::Replication);
    let creator = BucketCreator::new(&config, &bucket);

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    verify_versioning_enabled(&config, &bucket_name).await;
    verify_lifecycle_policy(&config, &bucket_name, "GLACIER").await;

    cleanup_bucket(&config, &bucket_name).await;
}

#[tokio::test]
#[ignore]
async fn test_create_bucket_pair_with_replication() {
    let config = test_config().await;
    let ts = timestamp();
    let primary_name = format!("{}-inttest-pair-{}", config.stack.as_str(), ts);
    let repl_name = format!("{}-inttest-pair-{}-repl", config.stack.as_str(), ts);

    let primary = Bucket(Name::new(&primary_name).unwrap(), Type::Standard);
    let replication = Bucket(Name::new(&repl_name).unwrap(), Type::Replication);

    let primary_creator = BucketCreator::new(&config, &primary);
    primary_creator
        .create()
        .await
        .expect("primary bucket creation failed");
    primary_creator
        .setup()
        .await
        .expect("primary bucket setup failed");

    let repl_creator = BucketCreator::new(&config, &replication);
    repl_creator
        .create()
        .await
        .expect("replication bucket creation failed");
    repl_creator
        .setup()
        .await
        .expect("replication bucket setup failed");

    primary_creator
        .enable_replication(&replication)
        .await
        .expect("enable replication failed");

    verify_replication_configured(&config, &primary_name, &repl_name).await;

    cleanup_bucket(&config, &primary_name).await;
    cleanup_bucket(&config, &repl_name).await;
}

#[tokio::test]
#[ignore]
async fn test_rollback_deletes_bucket() {
    let config = test_config().await;
    let bucket_name = format!("{}-inttest-rollback-{}", config.stack.as_str(), timestamp());
    let bucket = Bucket(Name::new(&bucket_name).unwrap(), Type::Standard);

    let creator = BucketCreator::new(&config, &bucket);
    creator.create().await.expect("bucket creation failed");

    assert!(
        bucket_exists(&config.s3_client, &bucket_name).await,
        "bucket should exist after creation"
    );

    creator.rollback().await.expect("rollback failed");

    assert!(
        !bucket_exists(&config.s3_client, &bucket_name).await,
        "bucket should not exist after rollback"
    );
}
