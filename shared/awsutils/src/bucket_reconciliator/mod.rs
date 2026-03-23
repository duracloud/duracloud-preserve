use std::fmt::Display;

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
    REPLICATION_RULE_ID, REPLICATION_RULE_PRIORITY, REPLICATION_TIME_MINUTES,
    STORAGE_CLASS_PUBLIC_DEFAULT, STORAGE_CLASS_REPLICATION_DEFAULT,
    STORAGE_CLASS_STANDARD_DEFAULT, STORAGE_TRANSITION_DAYS,
};

pub use crate::bucket_creator::BucketCreatorParams;
use crate::{bucket_policy, config};

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

impl Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepStatus::Ok => write!(f, "ok"),
            StepStatus::Drift => write!(f, "drift"),
            StepStatus::Error(_) => write!(f, "error"),
        }
    }
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
            && let Some(parsed) = config::parse_storage_class(val)
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
            if let Some(class) = config::parse_storage_class(id) {
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
                let expected = self.stack.logging_prefix_path(self.bucket.name());

                match resp.logging_enabled() {
                    Some(logging)
                        if logging.target_bucket() == expected.bucket()
                            && logging.target_prefix() == expected.key() =>
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

        let status = match bucket_policy::is_public_read_policy(&policy_str, self.bucket.name()) {
            Ok(true) => StepStatus::Ok,
            Ok(false) => StepStatus::Drift,
            Err(e) => StepStatus::Error(e),
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
