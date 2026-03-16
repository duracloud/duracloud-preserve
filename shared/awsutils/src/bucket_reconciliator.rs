use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::types::{
    BucketVersioningStatus, DeleteMarkerReplicationStatus, InventoryFrequency,
    InventoryIncludedObjectVersions, InventoryOptionalField, MetricsStatus, ReplicationRuleStatus,
    ReplicationTimeStatus, TransitionStorageClass,
};

use apputils::Stack;

use crate::bucket::{Bucket, Type};
use crate::bucket_creator::{
    BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY, EXPIRE_ABORTED_MULTIPART_DAYS,
    EXPIRE_NONCURRENT_VERSION_DAYS, INVENTORY_FORMAT, INVENTORY_ID, INVENTORY_PREFIX,
    LOGGING_PREFIX, REPLICATION_RULE_ID, REPLICATION_RULE_PRIORITY, REPLICATION_TIME_MINUTES,
    STORAGE_CLASS_PUBLIC_DEFAULT, STORAGE_CLASS_REPLICATION_DEFAULT,
    STORAGE_CLASS_STANDARD_DEFAULT, STORAGE_TRANSITION_DAYS,
};

pub use crate::bucket_creator::BucketCreatorParams;
use crate::config::parse_storage_class;

/// Status of a single reconciliation step.
#[derive(Debug)]
pub enum StepStatus {
    /// Configuration matches expected state.
    Ok,
    /// Configuration differs from expected state.
    Drift,
    /// An error occurred reading or comparing configuration.
    Error(String),
}

/// Result of a single reconciliation step.
#[derive(Debug)]
pub struct StepResult {
    pub name: &'static str,
    pub status: StepStatus,
}

/// Report for a single bucket's reconciliation.
#[derive(Debug)]
pub struct ReconcileReport {
    pub bucket_name: String,
    pub bucket_type: Type,
    pub steps: Vec<StepResult>,
}

impl ReconcileReport {
    pub fn has_errors(&self) -> bool {
        self.steps
            .iter()
            .any(|s| matches!(s.status, StepStatus::Error(_)))
    }

    pub fn has_drift(&self) -> bool {
        self.steps
            .iter()
            .any(|s| matches!(s.status, StepStatus::Drift))
    }
}

/// Reads and compares bucket configuration against expected state.
/// For now this is a reporter-only: no writes are performed.
pub struct BucketReconciliator<'a> {
    account_id: &'a str,
    bucket: &'a Bucket,
    client: &'a aws_sdk_s3::Client,
    replication_bucket: Option<&'a Bucket>,
    replication_role_arn: &'a str,
    stack: &'a Stack,
}

impl<'a> BucketReconciliator<'a> {
    pub fn new(
        params: &BucketCreatorParams<'a>,
        bucket: &'a Bucket,
        replication_bucket: Option<&'a Bucket>,
    ) -> Self {
        Self {
            account_id: params.account_id,
            bucket,
            client: params.client,
            replication_bucket,
            replication_role_arn: params.replication_role_arn,
            stack: params.stack,
        }
    }

    pub async fn reconcile(&self) -> ReconcileReport {
        let steps = match self.bucket.bucket_type() {
            Type::Standard => self.reconcile_standard().await,
            Type::Public => self.reconcile_public().await,
            Type::Replication => self.reconcile_replication().await,
            _ => vec![StepResult {
                name: "setup",
                status: StepStatus::Error(format!(
                    "reconciliation not supported for {} buckets",
                    self.bucket.bucket_type()
                )),
            }],
        };

        ReconcileReport {
            bucket_name: self.bucket.name().to_string(),
            bucket_type: *self.bucket.bucket_type(),
            steps,
        }
    }

    async fn reconcile_standard(&self) -> Vec<StepResult> {
        let (tag_result, resolved_class) = self.check_transition_class_tag().await;
        let mut steps = vec![tag_result];
        steps.push(self.check_versioning().await);
        steps.push(self.check_lifecycle(&resolved_class).await);
        steps.push(self.check_replication_config().await);
        steps.push(self.check_notifications().await);
        steps.push(self.check_logging().await);
        steps.push(self.check_inventory().await);
        steps
    }

    async fn reconcile_public(&self) -> Vec<StepResult> {
        let (tag_result, resolved_class) = self.check_transition_class_tag().await;
        let mut steps = vec![tag_result];
        steps.push(self.check_versioning().await);
        steps.push(self.check_lifecycle(&resolved_class).await);
        steps.push(self.check_replication_config().await);
        steps.push(self.check_notifications().await);
        steps.push(self.check_logging().await);
        steps.push(self.check_inventory().await);
        steps.push(self.check_public_access_block().await);
        steps.push(self.check_bucket_policy().await);
        steps
    }

    async fn reconcile_replication(&self) -> Vec<StepResult> {
        let (tag_result, resolved_class) = self.check_transition_class_tag().await;
        let mut steps = vec![tag_result];
        steps.push(self.check_versioning().await);
        steps.push(self.check_lifecycle(&resolved_class).await);
        steps
    }

    /// Read bucket tags and resolve the expected transition storage class.
    /// Returns the step result and the resolved class for downstream use.
    async fn check_transition_class_tag(&self) -> (StepResult, TransitionStorageClass) {
        let default_class = default_storage_class(self.bucket.bucket_type());

        let tags = match self
            .client
            .get_bucket_tagging()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => resp.tag_set().to_vec(),
            Err(e) if s3_error_has_code(&e, &["NoSuchTagSet"]) => {
                // No tags at all — resolve via default, report drift for missing tag.
                return (
                    StepResult {
                        name: "transition-tag",
                        status: StepStatus::Drift,
                    },
                    default_class,
                );
            }
            Err(e) => {
                return (
                    StepResult {
                        name: "transition-tag",
                        status: StepStatus::Error(s3_read_error("get bucket tagging", &e)),
                    },
                    default_class,
                );
            }
        };

        // Try to read existing TransitionStorageClass tag.
        let tag_value = tags
            .iter()
            .find(|t| t.key() == BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY)
            .map(|t| t.value().to_string());

        if let Some(ref val) = tag_value
            && let Some(parsed) = parse_storage_class(val)
        {
            return (
                StepResult {
                    name: "transition-tag",
                    status: StepStatus::Ok,
                },
                parsed,
            );
        }

        // Tag missing or invalid — try inference from lifecycle rules.
        let inferred = match self.infer_transition_class_from_lifecycle().await {
            Ok(v) => v,
            Err(msg) => {
                return (
                    StepResult {
                        name: "transition-tag",
                        status: StepStatus::Error(msg),
                    },
                    default_class,
                );
            }
        };
        let resolved = inferred.unwrap_or(default_class);

        (
            StepResult {
                name: "transition-tag",
                status: StepStatus::Drift,
            },
            resolved,
        )
    }

    /// Attempt to infer the transition storage class from existing lifecycle rules.
    /// The managed transition rule uses the storage class as_str() as the rule ID.
    async fn infer_transition_class_from_lifecycle(
        &self,
    ) -> Result<Option<TransitionStorageClass>, String> {
        let resp = match self
            .client
            .get_bucket_lifecycle_configuration()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) if s3_error_has_code(&e, &["NoSuchLifecycleConfiguration"]) => return Ok(None),
            Err(e) => {
                return Err(s3_read_error(
                    "get lifecycle configuration (for transition-tag inference)",
                    &e,
                ));
            }
        };

        for rule in resp.rules() {
            let Some(id) = rule.id() else { continue };
            // Skip the known expiration rule.
            if id == "ExpireOldVersions" {
                continue;
            }
            if let Some(class) = parse_storage_class(id) {
                return Ok(Some(class));
            }
        }

        Ok(None)
    }

    async fn check_versioning(&self) -> StepResult {
        let status = match self
            .client
            .get_bucket_versioning()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status() == Some(&BucketVersioningStatus::Enabled) {
                    StepStatus::Ok
                } else {
                    StepStatus::Drift
                }
            }
            Err(e) => StepStatus::Error(s3_read_error("get versioning", &e)),
        };

        StepResult {
            name: "versioning",
            status,
        }
    }

    async fn check_lifecycle(&self, expected_class: &TransitionStorageClass) -> StepResult {
        let rules = match self
            .client
            .get_bucket_lifecycle_configuration()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => resp.rules().to_vec(),
            Err(e) if s3_error_has_code(&e, &["NoSuchLifecycleConfiguration"]) => {
                // No lifecycle configuration at all.
                return StepResult {
                    name: "lifecycle",
                    status: StepStatus::Drift,
                };
            }
            Err(e) => {
                return StepResult {
                    name: "lifecycle",
                    status: StepStatus::Error(s3_read_error("get lifecycle configuration", &e)),
                };
            }
        };

        // Check the ExpireOldVersions rule.
        let expire_ok = rules.iter().any(|r| {
            r.id() == Some("ExpireOldVersions")
                && r.status() == &aws_sdk_s3::types::ExpirationStatus::Enabled
                && r.abort_incomplete_multipart_upload()
                    .and_then(|a| a.days_after_initiation())
                    == Some(EXPIRE_ABORTED_MULTIPART_DAYS as i32)
                && r.noncurrent_version_expiration()
                    .and_then(|n| n.noncurrent_days())
                    == Some(EXPIRE_NONCURRENT_VERSION_DAYS as i32)
                && r.expiration()
                    .and_then(|e| e.expired_object_delete_marker())
                    == Some(true)
        });

        // Check the transition rule (ID = storage class as_str()).
        let expected_id = expected_class.as_str();
        let transition_ok = rules.iter().any(|r| {
            r.id() == Some(expected_id)
                && r.status() == &aws_sdk_s3::types::ExpirationStatus::Enabled
                && r.transitions().len() == 1
                && r.transitions().first().is_some_and(|t| {
                    t.days() == Some(STORAGE_TRANSITION_DAYS as i32)
                        && t.storage_class().map(|c| c.as_str()) == Some(expected_id)
                })
        });

        StepResult {
            name: "lifecycle",
            status: if expire_ok && transition_ok {
                StepStatus::Ok
            } else {
                StepStatus::Drift
            },
        }
    }

    async fn check_replication_config(&self) -> StepResult {
        let Some(repl_bucket) = self.replication_bucket else {
            return StepResult {
                name: "replication",
                status: StepStatus::Error("no matching replication bucket found".to_string()),
            };
        };

        let repl_config = match self
            .client
            .get_bucket_replication()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e)
                if s3_error_has_code(
                    &e,
                    &[
                        "ReplicationConfigurationNotFoundError",
                        "ReplicationConfigurationNotFound",
                    ],
                ) =>
            {
                return StepResult {
                    name: "replication",
                    status: StepStatus::Drift,
                };
            }
            Err(e) => {
                return StepResult {
                    name: "replication",
                    status: StepStatus::Error(s3_read_error("get replication configuration", &e)),
                };
            }
        };

        let Some(config) = repl_config.replication_configuration() else {
            return StepResult {
                name: "replication",
                status: StepStatus::Drift,
            };
        };

        // Check role.
        if config.role() != self.replication_role_arn {
            return StepResult {
                name: "replication",
                status: StepStatus::Drift,
            };
        }

        let rules = config.rules();
        if rules.len() != 1 {
            return StepResult {
                name: "replication",
                status: StepStatus::Drift,
            };
        }

        let rule = &rules[0];
        let expected_dest_arn = format!("arn:aws:s3:::{}", repl_bucket.name());

        let ok = rule.id() == Some(REPLICATION_RULE_ID)
            && rule.status() == &ReplicationRuleStatus::Enabled
            && rule.priority() == Some(REPLICATION_RULE_PRIORITY)
            && rule.filter().and_then(|f| f.prefix()) == Some("")
            && rule.destination().is_some_and(|d| {
                d.bucket() == expected_dest_arn.as_str()
                    && d.replication_time().is_some_and(|rt| {
                        rt.status() == &ReplicationTimeStatus::Enabled
                            && rt.time().and_then(|t| t.minutes()) == Some(REPLICATION_TIME_MINUTES)
                    })
                    && d.metrics().is_some_and(|m| {
                        m.status() == &MetricsStatus::Enabled
                            && m.event_threshold().and_then(|t| t.minutes())
                                == Some(REPLICATION_TIME_MINUTES)
                    })
            })
            && rule.delete_marker_replication().and_then(|d| d.status())
                == Some(&DeleteMarkerReplicationStatus::Enabled);

        StepResult {
            name: "replication",
            status: if ok {
                StepStatus::Ok
            } else {
                StepStatus::Drift
            },
        }
    }

    async fn check_notifications(&self) -> StepResult {
        let status = match self
            .client
            .get_bucket_notification_configuration()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => {
                if resp.event_bridge_configuration().is_some() {
                    StepStatus::Ok
                } else {
                    StepStatus::Drift
                }
            }
            Err(e) => StepStatus::Error(s3_read_error("get notifications", &e)),
        };

        StepResult {
            name: "notifications",
            status,
        }
    }

    async fn check_logging(&self) -> StepResult {
        let status = match self
            .client
            .get_bucket_logging()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => {
                let expected_target = self.stack.managed_bucket();
                let expected_prefix = format!("{LOGGING_PREFIX}/{}/", self.bucket.name());

                match resp.logging_enabled() {
                    Some(logging)
                        if logging.target_bucket() == expected_target.as_str()
                            && logging.target_prefix() == expected_prefix.as_str() =>
                    {
                        StepStatus::Ok
                    }
                    _ => StepStatus::Drift,
                }
            }
            Err(e) => StepStatus::Error(s3_read_error("get logging", &e)),
        };

        StepResult {
            name: "logging",
            status,
        }
    }

    async fn check_inventory(&self) -> StepResult {
        let status = match self
            .client
            .get_bucket_inventory_configuration()
            .bucket(self.bucket.name())
            .id(INVENTORY_ID)
            .send()
            .await
        {
            Ok(resp) => {
                let Some(inv) = resp.inventory_configuration() else {
                    return StepResult {
                        name: "inventory",
                        status: StepStatus::Drift,
                    };
                };

                let managed_bucket_arn = format!("arn:aws:s3:::{}", self.stack.managed_bucket());

                let ok = inv.is_enabled()
                    && inv.id() == INVENTORY_ID
                    && inv.included_object_versions() == &InventoryIncludedObjectVersions::Current
                    && inv
                        .schedule()
                        .is_some_and(|s| s.frequency() == &InventoryFrequency::Daily)
                    && inv
                        .destination()
                        .and_then(|d| d.s3_bucket_destination())
                        .is_some_and(|d| {
                            d.bucket() == managed_bucket_arn.as_str()
                                && d.format() == &INVENTORY_FORMAT
                                && d.prefix() == Some(INVENTORY_PREFIX)
                                && d.account_id() == Some(self.account_id)
                        })
                    && inv
                        .optional_fields()
                        .contains(&InventoryOptionalField::Size)
                    && inv
                        .optional_fields()
                        .contains(&InventoryOptionalField::LastModifiedDate)
                    && inv
                        .optional_fields()
                        .contains(&InventoryOptionalField::StorageClass)
                    && inv
                        .optional_fields()
                        .contains(&InventoryOptionalField::ReplicationStatus);

                if ok {
                    StepStatus::Ok
                } else {
                    StepStatus::Drift
                }
            }
            Err(e)
                if s3_error_has_code(
                    &e,
                    &["NoSuchConfiguration", "NoSuchInventoryConfiguration"],
                ) =>
            {
                StepStatus::Drift
            }
            Err(e) => StepStatus::Error(s3_read_error("get inventory configuration", &e)),
        };

        StepResult {
            name: "inventory",
            status,
        }
    }

    async fn check_public_access_block(&self) -> StepResult {
        let status = match self
            .client
            .get_public_access_block()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => {
                let ok = resp.public_access_block_configuration().is_some_and(|pab| {
                    pab.block_public_acls() == Some(false)
                        && pab.ignore_public_acls() == Some(false)
                        && pab.block_public_policy() == Some(false)
                        && pab.restrict_public_buckets() == Some(false)
                });

                if ok {
                    StepStatus::Ok
                } else {
                    StepStatus::Drift
                }
            }
            Err(e)
                if s3_error_has_code(
                    &e,
                    &[
                        "NoSuchPublicAccessBlockConfiguration",
                        "NoSuchPublicAccessBlockConfigurationError",
                    ],
                ) =>
            {
                StepStatus::Drift
            }
            Err(e) => StepStatus::Error(s3_read_error("get public access block", &e)),
        };

        StepResult {
            name: "public-access",
            status,
        }
    }

    async fn check_bucket_policy(&self) -> StepResult {
        let policy_str = match self
            .client
            .get_bucket_policy()
            .bucket(self.bucket.name())
            .send()
            .await
        {
            Ok(resp) => match resp.policy() {
                Some(p) => p.to_string(),
                None => {
                    return StepResult {
                        name: "bucket-policy",
                        status: StepStatus::Drift,
                    };
                }
            },
            Err(e) if s3_error_has_code(&e, &["NoSuchBucketPolicy"]) => {
                return StepResult {
                    name: "bucket-policy",
                    status: StepStatus::Drift,
                };
            }
            Err(e) => {
                return StepResult {
                    name: "bucket-policy",
                    status: StepStatus::Error(s3_read_error("get bucket policy", &e)),
                };
            }
        };

        let status = match serde_json::from_str::<serde_json::Value>(&policy_str) {
            Ok(v) => {
                let expected_resource = format!("arn:aws:s3:::{}/*", self.bucket.name());

                let ok = v
                    .get("Statement")
                    .and_then(|s| s.as_array())
                    .is_some_and(|statements| {
                        statements.iter().any(|stmt| {
                            stmt.get("Sid").and_then(|s| s.as_str()) == Some("AllowPublicRead")
                                && stmt.get("Effect").and_then(|e| e.as_str()) == Some("Allow")
                                && stmt.get("Principal").and_then(|p| p.as_str()) == Some("*")
                                && stmt.get("Action").and_then(|a| a.as_str())
                                    == Some("s3:GetObject")
                                && stmt.get("Resource").and_then(|r| r.as_str())
                                    == Some(expected_resource.as_str())
                        })
                    });

                if ok {
                    StepStatus::Ok
                } else {
                    StepStatus::Drift
                }
            }
            Err(e) => StepStatus::Error(format!("failed to parse bucket policy: {e}")),
        };

        StepResult {
            name: "bucket-policy",
            status,
        }
    }
}

/// Resolve the default transition storage class for a bucket type.
fn default_storage_class(bucket_type: &Type) -> TransitionStorageClass {
    match bucket_type {
        Type::Public => STORAGE_CLASS_PUBLIC_DEFAULT,
        Type::Replication => STORAGE_CLASS_REPLICATION_DEFAULT,
        Type::Standard => STORAGE_CLASS_STANDARD_DEFAULT,
        _ => STORAGE_CLASS_STANDARD_DEFAULT,
    }
}

fn s3_error_has_code<E>(e: &E, codes: &[&str]) -> bool
where
    E: ProvideErrorMetadata,
{
    e.code().is_some_and(|code| codes.contains(&code))
}

fn s3_read_error<E>(operation: &str, e: &E) -> String
where
    E: ProvideErrorMetadata + std::fmt::Display,
{
    let code = e.code().unwrap_or("unknown");
    let message = e.message().unwrap_or("unknown");
    format!("{operation} failed (code={code}): {message}")
}

#[cfg(test)]
mod tests {
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
            parse_storage_class("GLACIER_IR"),
            Some(TransitionStorageClass::GlacierIr)
        ));
        assert!(matches!(
            parse_storage_class("DEEP_ARCHIVE"),
            Some(TransitionStorageClass::DeepArchive)
        ));
        assert!(matches!(
            parse_storage_class("INTELLIGENT_TIERING"),
            Some(TransitionStorageClass::IntelligentTiering)
        ));
        assert!(parse_storage_class("not-a-class").is_none());
        assert!(parse_storage_class("").is_none());
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
      <Prefix>{INVENTORY_PREFIX}</Prefix>
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
        serde_json::json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Sid": "AllowPublicRead",
                "Effect": "Allow",
                "Principal": "*",
                "Action": "s3:GetObject",
                "Resource": format!("arn:aws:s3:::{}/*", bucket_name)
            }]
        })
        .to_string()
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                logging_xml("wrong-bucket", &format!("{LOGGING_PREFIX}/{bucket_name}/")),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
                    &format!("{LOGGING_PREFIX}/{bucket_name}/"),
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
}
