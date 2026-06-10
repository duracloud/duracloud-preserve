//! Integration tests for bucket creation and configuration.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test -p awsutils --test bucket_creator -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "int-test")
//!   - Run: mise run setup --stack <stack> --profile <profile>

mod common;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::types::{
    BucketVersioningStatus, ExpirationStatus, InventoryFrequency, InventoryIncludedObjectVersions,
    InventoryOptionalField, TransitionStorageClass,
};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use awsutils::bucket::{self, Bucket, Type};
use awsutils::bucket_creator::INVENTORY_FORMAT;
use common::{
    IntegrationTestContext, bucket_creator, cleanup_bucket, integration_test_context, timestamp,
};
use constants::{
    EXPIRE_ABORTED_MULTIPART_DAYS, EXPIRE_NONCURRENT_VERSION_DAYS, STORAGE_TRANSITION_DAYS,
};

async fn verify_bucket_tags(ctx: &IntegrationTestContext, bucket: &str, expected_type: Type) {
    let result = ctx
        .s3
        .get_bucket_tagging()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get bucket tags");

    let tags = result.tag_set();
    let get_tag = |key: &str| tags.iter().find(|t| t.key() == key).map(|t| t.value());
    let expected_type = expected_type.to_string();

    assert_eq!(
        get_tag("BucketOrigin"),
        Some("bucket-request"),
        "BucketOrigin tag missing/incorrect on {}",
        bucket
    );
    assert_eq!(
        get_tag("Stack"),
        Some(ctx.stack.as_str()),
        "Stack tag missing/incorrect on {}",
        bucket
    );
    assert_eq!(
        get_tag("BucketType"),
        Some(expected_type.as_str()),
        "BucketType tag missing/incorrect on {}",
        bucket
    );
}

async fn verify_versioning_enabled(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
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

async fn verify_lifecycle_policy(ctx: &IntegrationTestContext, bucket: &str, expected_class: &str) {
    let result = ctx
        .s3
        .get_bucket_lifecycle_configuration()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get lifecycle");

    let rules = result.rules();

    let expire_rule = rules
        .iter()
        .find(|r| r.id() == Some("ExpireOldVersions"))
        .unwrap_or_else(|| panic!("missing ExpireOldVersions lifecycle rule on {}", bucket));

    assert_eq!(
        expire_rule.status(),
        &ExpirationStatus::Enabled,
        "ExpireOldVersions lifecycle rule not enabled on {}",
        bucket
    );
    assert_eq!(
        expire_rule
            .abort_incomplete_multipart_upload()
            .and_then(|r| r.days_after_initiation()),
        Some(EXPIRE_ABORTED_MULTIPART_DAYS as i32),
        "abort incomplete multipart days mismatch on {}",
        bucket
    );
    assert_eq!(
        expire_rule
            .noncurrent_version_expiration()
            .and_then(|r| r.noncurrent_days()),
        Some(EXPIRE_NONCURRENT_VERSION_DAYS as i32),
        "noncurrent version expiration days mismatch on {}",
        bucket
    );
    assert_eq!(
        expire_rule
            .expiration()
            .and_then(|r| r.expired_object_delete_marker()),
        Some(true),
        "expired object delete marker mismatch on {}",
        bucket
    );

    let transition_rule = rules
        .iter()
        .find(|r| r.id() == Some(expected_class))
        .unwrap_or_else(|| {
            panic!(
                "missing {} transition lifecycle rule on {}",
                expected_class, bucket
            )
        });

    assert_eq!(
        transition_rule.status(),
        &ExpirationStatus::Enabled,
        "transition lifecycle rule not enabled on {}",
        bucket
    );

    let transitions = transition_rule.transitions();
    assert_eq!(
        transitions.len(),
        1,
        "expected exactly 1 transition on {}",
        bucket
    );

    let transition = &transitions[0];
    assert_eq!(
        transition.days(),
        Some(STORAGE_TRANSITION_DAYS as i32),
        "transition days mismatch on {}",
        bucket
    );
    assert_eq!(
        transition
            .storage_class()
            .map(|c| c.as_str())
            .unwrap_or_default(),
        expected_class,
        "transition storage class mismatch on {}",
        bucket
    );
}

async fn verify_notifications_enabled(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
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

async fn verify_notifications_disabled(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
        .get_bucket_notification_configuration()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get notifications");

    assert!(
        result.event_bridge_configuration().is_none(),
        "EventBridge should not be configured on {}",
        bucket
    );
    assert!(
        result.lambda_function_configurations().is_empty(),
        "unexpected lambda notification configs on {}",
        bucket
    );
    assert!(
        result.queue_configurations().is_empty(),
        "unexpected queue notification configs on {}",
        bucket
    );
    assert!(
        result.topic_configurations().is_empty(),
        "unexpected topic notification configs on {}",
        bucket
    );
}

async fn verify_inventory_configured(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
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
    assert_eq!(inv.id(), "inventory", "inventory id mismatch on {}", bucket);
    assert_eq!(
        inv.included_object_versions(),
        &InventoryIncludedObjectVersions::Current,
        "inventory included versions mismatch on {}",
        bucket
    );

    let schedule = inv
        .schedule()
        .unwrap_or_else(|| panic!("inventory schedule missing on {}", bucket));
    assert_eq!(
        schedule.frequency(),
        &InventoryFrequency::Daily,
        "inventory frequency mismatch on {}",
        bucket
    );

    let dest = inv
        .destination()
        .and_then(|d| d.s3_bucket_destination())
        .unwrap_or_else(|| panic!("inventory destination missing on {}", bucket));

    let managed_bucket_arn = format!("arn:aws:s3:::{}", ctx.stack.managed_bucket());
    assert_eq!(
        dest.bucket(),
        managed_bucket_arn.as_str(),
        "inventory destination bucket mismatch on {}",
        bucket
    );
    assert_eq!(
        dest.format(),
        &INVENTORY_FORMAT,
        "inventory format mismatch on {}",
        bucket
    );
    assert_eq!(
        dest.prefix(),
        Some("manifests"),
        "inventory prefix mismatch on {}",
        bucket
    );
    assert_eq!(
        dest.account_id(),
        Some(ctx.account_id.as_str()),
        "inventory destination account id mismatch on {}",
        bucket
    );

    let optional_fields = inv.optional_fields();
    for field in [
        InventoryOptionalField::Size,
        InventoryOptionalField::LastModifiedDate,
        InventoryOptionalField::StorageClass,
        InventoryOptionalField::ReplicationStatus,
    ] {
        assert!(
            optional_fields.contains(&field),
            "inventory optional field {field:?} missing on {}",
            bucket
        );
    }
}

async fn verify_inventory_not_configured(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
        .get_bucket_inventory_configuration()
        .bucket(bucket)
        .id("inventory")
        .send()
        .await;

    assert!(
        result.is_err(),
        "unexpected inventory configuration on {}",
        bucket
    );
}

async fn verify_logging_enabled(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
        .get_bucket_logging()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get logging");

    let logging = result
        .logging_enabled()
        .unwrap_or_else(|| panic!("logging not enabled on {}", bucket));
    let managed_bucket = ctx.stack.managed_bucket();
    assert_eq!(
        logging.target_bucket(),
        managed_bucket.as_str(),
        "unexpected logging target bucket on {}",
        bucket
    );
    let expected_prefix = format!("audit/{}/", bucket);
    assert_eq!(
        logging.target_prefix(),
        expected_prefix.as_str(),
        "unexpected logging prefix on {}",
        bucket
    );
}

async fn verify_logging_disabled(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
        .get_bucket_logging()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get logging");

    assert!(
        result.logging_enabled().is_none(),
        "unexpected logging enabled on {}",
        bucket
    );
}

async fn verify_replication_configured(ctx: &IntegrationTestContext, src: &str, dest: &str) {
    let result = ctx
        .s3
        .get_bucket_replication()
        .bucket(src)
        .send()
        .await
        .expect("failed to get replication");

    let repl_config = result
        .replication_configuration()
        .expect("no replication config");
    assert_eq!(
        repl_config.role(),
        ctx.replication_role_arn.as_str(),
        "unexpected replication role on {}",
        src
    );
    let rules = repl_config.rules();
    assert_eq!(
        rules.len(),
        1,
        "expected exactly 1 replication rule on {}",
        src
    );

    let dest_arn = format!("arn:aws:s3:::{}", dest);
    let rule = &rules[0];

    assert_eq!(
        rule.id(),
        Some("ReplicateAll"),
        "unexpected replication rule id on {}",
        src
    );
    assert_eq!(
        rule.status(),
        &aws_sdk_s3::types::ReplicationRuleStatus::Enabled,
        "replication rule not enabled on {}",
        src
    );
    assert_eq!(
        rule.priority(),
        Some(1),
        "unexpected replication rule priority on {}",
        src
    );
    assert_eq!(
        rule.filter().and_then(|f| f.prefix()),
        Some(""),
        "unexpected replication filter prefix on {}",
        src
    );

    let destination = rule
        .destination()
        .unwrap_or_else(|| panic!("missing replication destination on {}", src));
    assert_eq!(
        destination.bucket(),
        dest_arn.as_str(),
        "replication destination mismatch on {}",
        src
    );

    let replication_time = destination
        .replication_time()
        .unwrap_or_else(|| panic!("missing replication time on {}", src));
    assert_eq!(
        replication_time.status(),
        &aws_sdk_s3::types::ReplicationTimeStatus::Enabled,
        "replication time not enabled on {}",
        src
    );
    assert_eq!(
        replication_time.time().and_then(|t| t.minutes()),
        Some(15),
        "unexpected replication time minutes on {}",
        src
    );

    let metrics = destination
        .metrics()
        .unwrap_or_else(|| panic!("missing replication metrics on {}", src));
    assert_eq!(
        metrics.status(),
        &aws_sdk_s3::types::MetricsStatus::Enabled,
        "replication metrics not enabled on {}",
        src
    );
    assert_eq!(
        metrics.event_threshold().and_then(|t| t.minutes()),
        Some(15),
        "unexpected replication metrics threshold minutes on {}",
        src
    );

    let delete_marker_replication = rule
        .delete_marker_replication()
        .unwrap_or_else(|| panic!("missing delete marker replication on {}", src));
    assert_eq!(
        delete_marker_replication.status(),
        Some(&aws_sdk_s3::types::DeleteMarkerReplicationStatus::Enabled),
        "delete marker replication not enabled on {}",
        src
    );
}

async fn verify_public_access_block_disabled(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
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
        pab.ignore_public_acls(),
        Some(false),
        "ignore_public_acls should be false on {}",
        bucket
    );
    assert_eq!(
        pab.block_public_policy(),
        Some(false),
        "block_public_policy should be false on {}",
        bucket
    );
    assert_eq!(
        pab.restrict_public_buckets(),
        Some(false),
        "restrict_public_buckets should be false on {}",
        bucket
    );
}

async fn verify_public_read_policy(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx
        .s3
        .get_bucket_policy()
        .bucket(bucket)
        .send()
        .await
        .expect("failed to get bucket policy");

    let policy = result.policy().expect("no policy");
    let v: serde_json::Value = serde_json::from_str(policy).expect("bucket policy invalid json");
    let statements = v
        .get("Statement")
        .and_then(|s| s.as_array())
        .unwrap_or_else(|| panic!("bucket policy missing Statement array on {}", bucket));

    let allow = statements
        .iter()
        .find(|s| s.get("Sid").and_then(|sid| sid.as_str()) == Some("AllowPublicRead"))
        .unwrap_or_else(|| panic!("AllowPublicRead statement missing on {}", bucket));

    assert_eq!(
        allow.get("Effect").and_then(|e| e.as_str()),
        Some("Allow"),
        "AllowPublicRead effect mismatch on {}",
        bucket
    );
    assert_eq!(
        allow.get("Principal").and_then(|p| p.as_str()),
        Some("*"),
        "AllowPublicRead principal mismatch on {}",
        bucket
    );
    assert_eq!(
        allow.get("Action").and_then(|a| a.as_str()),
        Some("s3:GetObject"),
        "AllowPublicRead action mismatch on {}",
        bucket
    );

    let expected_resource = format!("arn:aws:s3:::{}/*", bucket);
    assert_eq!(
        allow.get("Resource").and_then(|r| r.as_str()),
        Some(expected_resource.as_str()),
        "AllowPublicRead resource mismatch on {}",
        bucket
    );

    assert!(
        !policy.contains("DenyAllUploads"),
        "DenyAllUploads should not be present in final policy on {}",
        bucket
    );
}

async fn verify_no_bucket_policy(ctx: &IntegrationTestContext, bucket: &str) {
    let result = ctx.s3.get_bucket_policy().bucket(bucket).send().await;
    let err = result.unwrap_err();

    match err {
        SdkError::ServiceError(service_err) => {
            assert_eq!(
                service_err.err().code(),
                Some("NoSuchBucketPolicy"),
                "expected NoSuchBucketPolicy for {}, got {:?}",
                bucket,
                service_err.err().code()
            );
        }
        other => panic!("expected service error for {}, got {:?}", bucket, other),
    }
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

    verify_bucket_tags(&ctx, &bucket_name, Type::Standard).await;
    verify_versioning_enabled(&ctx, &bucket_name).await;
    verify_lifecycle_policy(&ctx, &bucket_name, "GLACIER_IR").await;
    verify_notifications_enabled(&ctx, &bucket_name).await;
    verify_inventory_configured(&ctx, &bucket_name).await;
    verify_logging_enabled(&ctx, &bucket_name).await;
    verify_no_bucket_policy(&ctx, &bucket_name).await;

    cleanup_bucket(&ctx.s3, &bucket_name).await;
}

#[tokio::test]
#[ignore]
async fn test_create_public_bucket() {
    let ctx = integration_test_context().await;
    let bucket_name = format!("{}-inttest-pub-{}-public", ctx.stack.as_str(), timestamp());

    let bucket = Bucket::new(&bucket_name, Type::Public).unwrap();
    let creator = bucket_creator(&ctx, &bucket, None);

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    verify_bucket_tags(&ctx, &bucket_name, Type::Public).await;
    verify_versioning_enabled(&ctx, &bucket_name).await;
    verify_lifecycle_policy(&ctx, &bucket_name, "INTELLIGENT_TIERING").await;
    verify_notifications_enabled(&ctx, &bucket_name).await;
    verify_inventory_configured(&ctx, &bucket_name).await;
    verify_logging_enabled(&ctx, &bucket_name).await;
    verify_public_access_block_disabled(&ctx, &bucket_name).await;
    verify_public_read_policy(&ctx, &bucket_name).await;

    cleanup_bucket(&ctx.s3, &bucket_name).await;
}

#[tokio::test]
#[ignore]
async fn test_create_replication_bucket() {
    let ctx = integration_test_context().await;
    let bucket_name = format!("{}-inttest-repl-{}-repl", ctx.stack.as_str(), timestamp());

    let bucket = Bucket::new(&bucket_name, Type::Replication).unwrap();
    let creator = bucket_creator(&ctx, &bucket, None);

    creator.create().await.expect("bucket creation failed");
    creator.setup().await.expect("bucket setup failed");

    verify_bucket_tags(&ctx, &bucket_name, Type::Replication).await;
    verify_versioning_enabled(&ctx, &bucket_name).await;
    verify_lifecycle_policy(&ctx, &bucket_name, "DEEP_ARCHIVE").await;
    verify_notifications_disabled(&ctx, &bucket_name).await;
    verify_logging_disabled(&ctx, &bucket_name).await;
    verify_inventory_not_configured(&ctx, &bucket_name).await;
    verify_no_bucket_policy(&ctx, &bucket_name).await;

    cleanup_bucket(&ctx.s3, &bucket_name).await;
}

#[tokio::test]
#[ignore]
async fn test_create_standard_bucket_pair_with_replication() {
    let ctx = integration_test_context().await;
    let ts = timestamp();
    let primary_name = format!("{}-inttest-pair-{}", ctx.stack.as_str(), ts);
    let repl_name = format!("{}-inttest-pair-{}-repl", ctx.stack.as_str(), ts);

    let primary = Bucket::new(&primary_name, Type::Standard).unwrap();
    let replication = Bucket::new(&repl_name, Type::Replication).unwrap();

    let primary_creator = bucket_creator(&ctx, &primary, Some(TransitionStorageClass::GlacierIr));
    primary_creator
        .create()
        .await
        .expect("primary bucket creation failed");
    primary_creator
        .setup()
        .await
        .expect("primary bucket setup failed");

    let repl_creator = bucket_creator(&ctx, &replication, None);
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

    verify_replication_configured(&ctx, &primary_name, &repl_name).await;

    cleanup_bucket(&ctx.s3, &primary_name).await;
    cleanup_bucket(&ctx.s3, &repl_name).await;
}

#[tokio::test]
#[ignore]
async fn test_create_public_bucket_pair_with_replication() {
    let ctx = integration_test_context().await;
    let ts = timestamp();
    let primary_name = format!("{}-inttest-pairpub-{}-public", ctx.stack.as_str(), ts);
    let repl_name = format!("{}-inttest-pairpub-{}-public-repl", ctx.stack.as_str(), ts);

    let primary = Bucket::new(&primary_name, Type::Public).unwrap();
    let replication = Bucket::new(&repl_name, Type::Replication).unwrap();

    let primary_creator = bucket_creator(&ctx, &primary, None);
    primary_creator
        .create()
        .await
        .expect("primary bucket creation failed");
    primary_creator
        .setup()
        .await
        .expect("primary bucket setup failed");

    let repl_creator = bucket_creator(&ctx, &replication, None);
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

    verify_public_access_block_disabled(&ctx, &primary_name).await;
    verify_public_read_policy(&ctx, &primary_name).await;
    verify_replication_configured(&ctx, &primary_name, &repl_name).await;

    cleanup_bucket(&ctx.s3, &primary_name).await;
    cleanup_bucket(&ctx.s3, &repl_name).await;
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
        bucket::exists(&ctx.s3, &bucket_name)
            .await
            .expect("exists check failed"),
        "bucket should exist after creation"
    );

    creator.rollback().await.expect("rollback failed");

    assert!(
        !bucket::exists(&ctx.s3, &bucket_name)
            .await
            .expect("exists check failed"),
        "bucket should not exist after rollback"
    );
}
