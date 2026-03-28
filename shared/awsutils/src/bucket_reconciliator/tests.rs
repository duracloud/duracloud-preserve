use super::*;

#[test]
fn test_default_storage_class() {
    assert_eq!(
        default_storage_class(&Type::Standard).as_str(),
        STORAGE_CLASS_STANDARD_DEFAULT.as_str()
    );
    assert_eq!(
        default_storage_class(&Type::Public).as_str(),
        STORAGE_CLASS_PUBLIC_DEFAULT.as_str()
    );
    assert_eq!(
        default_storage_class(&Type::Replication).as_str(),
        STORAGE_CLASS_REPLICATION_DEFAULT.as_str()
    );
    assert_eq!(
        default_storage_class(&Type::Internal).as_str(),
        STORAGE_CLASS_STANDARD_DEFAULT.as_str()
    );
}

#[test]
fn test_parse_transition_storage_class() {
    assert!(matches!(
        config::parse_storage_class("GLACIER_IR"),
        Some(TransitionStorageClass::GlacierIr)
    ));
    assert!(matches!(
        config::parse_storage_class("DEEP_ARCHIVE"),
        Some(TransitionStorageClass::DeepArchive)
    ));
    assert!(matches!(
        config::parse_storage_class("INTELLIGENT_TIERING"),
        Some(TransitionStorageClass::IntelligentTiering)
    ));
    assert!(config::parse_storage_class("not-a-class").is_none());
    assert!(config::parse_storage_class("").is_none());
}

#[tokio::test]
async fn test_reconcile_internal_bucket_unsupported() {
    let client = TestClientBuilder::new().build();
    let stack = test_stack();
    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new("test-stack-internal", Type::Internal).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, None);
    let report = reconciliator.reconcile().await;

    assert!(report.has_errors());
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].name, "setup");
    assert!(matches!(report.steps[0].status, StepStatus::Error(_)));
}

#[tokio::test]
async fn test_reconcile_public_access_blocked() {
    let stack = test_stack();
    let bucket_name = "test-stack-data-public";
    let repl_name = "test-stack-data-public-repl";
    let class = STORAGE_CLASS_PUBLIC_DEFAULT.as_str();

    let blocked_pab = r#"<?xml version="1.0" encoding="UTF-8"?>
<PublicAccessBlockConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <BlockPublicAcls>true</BlockPublicAcls>
  <IgnorePublicAcls>true</IgnorePublicAcls>
  <BlockPublicPolicy>true</BlockPublicPolicy>
  <RestrictPublicBuckets>true</RestrictPublicBuckets>
</PublicAccessBlockConfiguration>"#;

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        // 8. get_public_access_block (all blocked → drift)
        .success(blocked_pab, None)
        // 9. get_bucket_policy
        .success(bucket_policy_json_public_read(bucket_name), None)
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Public).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert!(report.has_drift());
    let pab = report
        .steps
        .iter()
        .find(|s| s.name == "public-access")
        .unwrap();
    assert!(matches!(pab.status, StepStatus::Drift));
}

#[tokio::test]
async fn test_reconcile_public_all_ok() {
    let stack = test_stack();
    let bucket_name = "test-stack-data-public";
    let repl_name = "test-stack-data-public-repl";
    let class = STORAGE_CLASS_PUBLIC_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        // 8. get_public_access_block
        .success(public_access_block_xml_open(), None)
        // 9. get_bucket_policy
        .success(bucket_policy_json_public_read(bucket_name), None)
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Public).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert_eq!(report.steps.len(), 9);
    assert!(!report.has_errors());
    assert!(!report.has_drift());
}

#[tokio::test]
async fn test_reconcile_public_missing_policy() {
    let stack = test_stack();
    let bucket_name = "test-stack-data-public";
    let repl_name = "test-stack-data-public-repl";
    let class = STORAGE_CLASS_PUBLIC_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        // 8. get_public_access_block
        .success(public_access_block_xml_open(), None)
        // 9. get_bucket_policy (error → drift)
        .error(
            404,
            "NoSuchBucketPolicy",
            "The bucket policy does not exist",
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Public).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert!(report.has_drift());
    let policy = report
        .steps
        .iter()
        .find(|s| s.name == "bucket-policy")
        .unwrap();
    assert!(matches!(policy.status, StepStatus::Drift));
}

#[tokio::test]
async fn test_reconcile_replication_bucket() {
    let class = STORAGE_CLASS_REPLICATION_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        .build();

    let stack = test_stack();
    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new("test-stack-example-repl", Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, None);
    let report = reconciliator.reconcile().await;

    assert_eq!(report.steps.len(), 3);
    assert!(!report.has_errors());
    assert!(!report.has_drift());

    let step_names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert_eq!(
        step_names,
        vec!["transition-tag", "versioning", "lifecycle"]
    );
}

#[test]
fn test_reconcile_report_all_ok() {
    let report = ReconcileReport {
        bucket_name: "test".to_string(),
        bucket_type: Type::Standard,
        steps: vec![StepResult {
            name: "versioning",
            status: StepStatus::Ok,
        }],
    };
    assert!(!report.has_errors());
    assert!(!report.has_drift());
}

// --- Mocked AWS SDK tests ---

use test_support::TestClientBuilder;

const TEST_ACCOUNT_ID: &str = "123456789012";
const TEST_REPL_ROLE_ARN: &str = "arn:aws:iam::123456789012:role/test-replication-role";

fn test_stack() -> Stack {
    Stack::new("test-stack").unwrap()
}

// --- XML response builders ---

fn tagging_xml(tags: &[(&str, &str)]) -> String {
    let tag_xml: String = tags
        .iter()
        .map(|(k, v)| format!("<Tag><Key>{k}</Key><Value>{v}</Value></Tag>"))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet>{tag_xml}</TagSet>
</Tagging>"#
    )
}

fn versioning_xml(status: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Status>{status}</Status>
</VersioningConfiguration>"#
    )
}

fn lifecycle_xml_full(transition_class: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<LifecycleConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule>
    <ID>ExpireOldVersions</ID>
    <Status>Enabled</Status>
    <Filter><Prefix></Prefix></Filter>
    <AbortIncompleteMultipartUpload>
      <DaysAfterInitiation>{}</DaysAfterInitiation>
    </AbortIncompleteMultipartUpload>
    <NoncurrentVersionExpiration>
      <NoncurrentDays>{}</NoncurrentDays>
    </NoncurrentVersionExpiration>
    <Expiration>
      <ExpiredObjectDeleteMarker>true</ExpiredObjectDeleteMarker>
    </Expiration>
  </Rule>
  <Rule>
    <ID>{transition_class}</ID>
    <Status>Enabled</Status>
    <Filter><Prefix></Prefix></Filter>
    <Transition>
      <Days>{}</Days>
      <StorageClass>{transition_class}</StorageClass>
    </Transition>
  </Rule>
</LifecycleConfiguration>"#,
        EXPIRE_ABORTED_MULTIPART_DAYS, EXPIRE_NONCURRENT_VERSION_DAYS, STORAGE_TRANSITION_DAYS,
    )
}

fn replication_xml(dest_bucket: &str, role_arn: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ReplicationConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Role>{role_arn}</Role>
  <Rule>
    <ID>{REPLICATION_RULE_ID}</ID>
    <Status>Enabled</Status>
    <Priority>{REPLICATION_RULE_PRIORITY}</Priority>
    <Filter><Prefix></Prefix></Filter>
    <Destination>
      <Bucket>arn:aws:s3:::{dest_bucket}</Bucket>
      <ReplicationTime>
        <Status>Enabled</Status>
        <Time><Minutes>{REPLICATION_TIME_MINUTES}</Minutes></Time>
      </ReplicationTime>
      <Metrics>
        <Status>Enabled</Status>
        <EventThreshold><Minutes>{REPLICATION_TIME_MINUTES}</Minutes></EventThreshold>
      </Metrics>
    </Destination>
    <DeleteMarkerReplication>
      <Status>Enabled</Status>
    </DeleteMarkerReplication>
  </Rule>
</ReplicationConfiguration>"#
    )
}

fn notification_xml_with_eventbridge() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<NotificationConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <EventBridgeConfiguration></EventBridgeConfiguration>
</NotificationConfiguration>"#
}

fn logging_xml(target_bucket: &str, target_prefix: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<BucketLoggingStatus xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <LoggingEnabled>
    <TargetBucket>{target_bucket}</TargetBucket>
    <TargetPrefix>{target_prefix}</TargetPrefix>
  </LoggingEnabled>
</BucketLoggingStatus>"#
    )
}

fn inventory_xml(dest_bucket_arn: &str, account_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<InventoryConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Id>{INVENTORY_ID}</Id>
  <IsEnabled>true</IsEnabled>
  <IncludedObjectVersions>Current</IncludedObjectVersions>
  <Schedule><Frequency>Daily</Frequency></Schedule>
  <Destination>
    <S3BucketDestination>
      <AccountId>{account_id}</AccountId>
      <Bucket>{dest_bucket_arn}</Bucket>
      <Format>Parquet</Format>
      <Prefix>{MANIFESTS_PREFIX}</Prefix>
    </S3BucketDestination>
  </Destination>
  <OptionalFields>
    <Field>Size</Field>
    <Field>LastModifiedDate</Field>
    <Field>StorageClass</Field>
    <Field>ReplicationStatus</Field>
  </OptionalFields>
</InventoryConfiguration>"#
    )
}

fn public_access_block_xml_open() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<PublicAccessBlockConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <BlockPublicAcls>false</BlockPublicAcls>
  <IgnorePublicAcls>false</IgnorePublicAcls>
  <BlockPublicPolicy>false</BlockPublicPolicy>
  <RestrictPublicBuckets>false</RestrictPublicBuckets>
</PublicAccessBlockConfiguration>"#
}

fn bucket_policy_json_public_read(bucket_name: &str) -> String {
    crate::bucket_policy::public_read_policy(bucket_name)
}

/// Helper: build a client with all the responses for a fully configured standard bucket.
/// Call order: get_bucket_tagging, get_bucket_versioning, get_bucket_lifecycle_configuration,
/// get_bucket_replication, get_bucket_notification_configuration,
/// get_bucket_logging, get_bucket_inventory_configuration.
fn standard_all_ok_client(
    bucket_name: &str,
    repl_bucket_name: &str,
    transition_class: &str,
) -> aws_sdk_s3::Client {
    let stack = test_stack();
    TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, transition_class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(transition_class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_bucket_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build()
}

#[test]
fn test_reconcile_report_has_drift() {
    let report = ReconcileReport {
        bucket_name: "test".to_string(),
        bucket_type: Type::Standard,
        steps: vec![
            StepResult {
                name: "versioning",
                status: StepStatus::Ok,
            },
            StepResult {
                name: "lifecycle",
                status: StepStatus::Drift,
            },
        ],
    };
    assert!(!report.has_errors());
    assert!(report.has_drift());
}

#[test]
fn test_reconcile_report_has_errors() {
    let report = ReconcileReport {
        bucket_name: "test".to_string(),
        bucket_type: Type::Standard,
        steps: vec![
            StepResult {
                name: "versioning",
                status: StepStatus::Ok,
            },
            StepResult {
                name: "lifecycle",
                status: StepStatus::Error("fail".to_string()),
            },
        ],
    };
    assert!(report.has_errors());
    assert!(!report.has_drift());
}

#[tokio::test]
async fn test_reconcile_standard_all_ok() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let repl_name = "test-stack-example-repl";
    let class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let client = standard_all_ok_client(bucket_name, repl_name, class);
    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert_eq!(report.bucket_name, bucket_name);
    assert!(!report.has_errors());
    for step in &report.steps {
        assert!(
            matches!(step.status, StepStatus::Ok),
            "step '{}' expected Ok, got {:?}",
            step.name,
            step.status,
        );
    }
    assert!(!report.has_drift());
    assert_eq!(report.steps.len(), 7);
}

#[tokio::test]
async fn test_reconcile_standard_lifecycle_drift_missing_config() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let repl_name = "test-stack-example-repl";
    let class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration (error → drift)
        .error(
            404,
            "NoSuchLifecycleConfiguration",
            "The lifecycle configuration does not exist",
        )
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert!(report.has_drift());
    let lifecycle = report.steps.iter().find(|s| s.name == "lifecycle").unwrap();
    assert!(matches!(lifecycle.status, StepStatus::Drift));
}

#[tokio::test]
async fn test_reconcile_standard_logging_wrong_target() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let repl_name = "test-stack-example-repl";
    let class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging (wrong target bucket)
        .success(
            logging_xml("wrong-bucket", stack.logging_prefix_path(bucket_name).key()),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert!(report.has_drift());
    let logging = report.steps.iter().find(|s| s.name == "logging").unwrap();
    assert!(matches!(logging.status, StepStatus::Drift));
}

#[tokio::test]
async fn test_reconcile_standard_missing_tag_inferred_from_lifecycle() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let repl_name = "test-stack-example-repl";
    let class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging (no transition tag → drift)
        .success(tagging_xml(&[("OtherTag", "value")]), None)
        // 1b. infer_transition_class_from_lifecycle → get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration (second call for check_lifecycle)
        .success(lifecycle_xml_full(class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert!(report.has_drift());
    assert!(!report.has_errors());

    // Transition tag reports drift (missing).
    let tag_step = report
        .steps
        .iter()
        .find(|s| s.name == "transition-tag")
        .unwrap();
    assert!(matches!(tag_step.status, StepStatus::Drift));

    // Lifecycle should be Ok (inferred class matches existing rules).
    let lifecycle = report.steps.iter().find(|s| s.name == "lifecycle").unwrap();
    assert!(matches!(lifecycle.status, StepStatus::Ok));
}

#[tokio::test]
async fn test_reconcile_standard_missing_tag_no_lifecycle_falls_back_to_default() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let repl_name = "test-stack-example-repl";
    let default_class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging (error → no tags)
        .error(404, "NoSuchTagSet", "The TagSet does not exist")
        // No infer call since tagging error means no tag_value path.
        // After error the code returns Drift + default_class directly (no lifecycle inference).
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration (for check_lifecycle, returns full rules with default class)
        .success(lifecycle_xml_full(default_class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    // transition-tag drift, everything else ok.
    let tag_step = report
        .steps
        .iter()
        .find(|s| s.name == "transition-tag")
        .unwrap();
    assert!(matches!(tag_step.status, StepStatus::Drift));

    let lifecycle = report.steps.iter().find(|s| s.name == "lifecycle").unwrap();
    assert!(
        matches!(lifecycle.status, StepStatus::Ok),
        "lifecycle expected Ok with default class, got {:?}",
        lifecycle.status,
    );
}

#[tokio::test]
async fn test_reconcile_standard_no_replication_bucket() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 4. check_replication_config returns Error immediately (no repl bucket) — no S3 call
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    // No replication bucket provided.
    let reconciliator = BucketReconciliator::new(&params, &bucket, None);
    let report = reconciliator.reconcile().await;

    assert!(report.has_errors());
    let repl_step = report
        .steps
        .iter()
        .find(|s| s.name == "replication")
        .unwrap();
    assert!(matches!(repl_step.status, StepStatus::Error(_)));
}

#[tokio::test]
async fn test_reconcile_standard_notifications_missing_eventbridge() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let repl_name = "test-stack-example-repl";
    let class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let no_eventbridge = r#"<?xml version="1.0" encoding="UTF-8"?>
<NotificationConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
</NotificationConfiguration>"#;

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration (no EventBridge)
        .success(no_eventbridge, None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert!(report.has_drift());
    let notif = report
        .steps
        .iter()
        .find(|s| s.name == "notifications")
        .unwrap();
    assert!(matches!(notif.status, StepStatus::Drift));
}

#[tokio::test]
async fn test_reconcile_standard_replication_drift_wrong_role() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let repl_name = "test-stack-example-repl";
    let class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning
        .success(versioning_xml("Enabled"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 4. get_bucket_replication (wrong role ARN)
        .success(
            replication_xml(repl_name, "arn:aws:iam::999999999999:role/wrong-role"),
            None,
        )
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert!(report.has_drift());
    let repl_step = report
        .steps
        .iter()
        .find(|s| s.name == "replication")
        .unwrap();
    assert!(matches!(repl_step.status, StepStatus::Drift));
}

#[tokio::test]
async fn test_reconcile_standard_versioning_suspended() {
    let stack = test_stack();
    let bucket_name = "test-stack-example";
    let repl_name = "test-stack-example-repl";
    let class = STORAGE_CLASS_STANDARD_DEFAULT.as_str();

    let client = TestClientBuilder::new()
        // 1. get_bucket_tagging (with valid tag)
        .success(
            tagging_xml(&[(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, class)]),
            None,
        )
        // 2. get_bucket_versioning (suspended)
        .success(versioning_xml("Suspended"), None)
        // 3. get_bucket_lifecycle_configuration
        .success(lifecycle_xml_full(class), None)
        // 4. get_bucket_replication
        .success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        // 5. get_bucket_notification_configuration
        .success(notification_xml_with_eventbridge(), None)
        // 6. get_bucket_logging
        .success(
            logging_xml(
                &stack.managed_bucket(),
                stack.logging_prefix_path(bucket_name).key(),
            ),
            None,
        )
        // 7. get_bucket_inventory_configuration
        .success(
            inventory_xml(
                &format!("arn:aws:s3:::{}", stack.managed_bucket()),
                TEST_ACCOUNT_ID,
            ),
            None,
        )
        .build();

    let params = BucketCreatorParams {
        account_id: TEST_ACCOUNT_ID,
        client: &client,
        replication_role_arn: TEST_REPL_ROLE_ARN,
        stack: &stack,
    };

    let bucket = Bucket::new(bucket_name, Type::Standard).unwrap();
    let repl_bucket = Bucket::new(repl_name, Type::Replication).unwrap();
    let reconciliator = BucketReconciliator::new(&params, &bucket, Some(&repl_bucket));
    let report = reconciliator.reconcile().await;

    assert!(report.has_drift());
    assert!(!report.has_errors());

    let versioning = report
        .steps
        .iter()
        .find(|s| s.name == "versioning")
        .unwrap();
    assert!(matches!(versioning.status, StepStatus::Drift));

    // All other steps should be Ok.
    for step in &report.steps {
        if step.name != "versioning" {
            assert!(
                matches!(step.status, StepStatus::Ok),
                "step '{}' expected Ok, got {:?}",
                step.name,
                step.status,
            );
        }
    }
}
