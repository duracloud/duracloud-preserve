use aws_sdk_s3::types::{
    AbortIncompleteMultipartUpload, BucketLifecycleConfiguration, BucketLocationConstraint,
    BucketVersioningStatus, CreateBucketConfiguration, DeleteMarkerReplication,
    DeleteMarkerReplicationStatus, Destination, EventBridgeConfiguration, ExpirationStatus,
    InventoryConfiguration, InventoryDestination, InventoryFormat, InventoryFrequency,
    InventoryIncludedObjectVersions, InventoryOptionalField, InventoryS3BucketDestination,
    InventorySchedule, LifecycleExpiration, LifecycleRule, LifecycleRuleFilter, Metrics,
    MetricsStatus, NoncurrentVersionExpiration, NotificationConfiguration,
    ReplicationConfiguration, ReplicationRule, ReplicationRuleFilter, ReplicationRuleStatus,
    ReplicationTime, ReplicationTimeStatus, ReplicationTimeValue, Tag, Transition,
    TransitionStorageClass, VersioningConfiguration,
};

use crate::bucket::{BUCKET_TAG_STACK_KEY, BUCKET_TAG_TYPE_KEY, Bucket, RequestError, Type};
use crate::config::Config;
use crate::config::get_region;

const BUCKET_TAG_ORIGIN_KEY: &str = "BucketOrigin";
const BUCKET_TAG_ORIGIN_VAL: &str = "bucket-request";

const EXPIRE_ABORTED_MULTIPART_DAYS: u8 = 3;
const EXPIRE_NONCURRENT_VERSION_DAYS: u8 = 14;

pub const INVENTORY_FORMAT: InventoryFormat = InventoryFormat::Parquet;
const INVENTORY_ID: &str = "inventory";
const INVENTORY_PREFIX: &str = "manifests";

const STORAGE_CLASS_PUBLIC: TransitionStorageClass = TransitionStorageClass::IntelligentTiering;
const STORAGE_CLASS_REPLICATION: TransitionStorageClass = TransitionStorageClass::DeepArchive;
const STORAGE_CLASS_STANDARD: TransitionStorageClass = TransitionStorageClass::GlacierIr;
const STORAGE_TRANSITION_DAYS: u8 = 7;

/// Handles bucket setup by delegating to the appropriate methods per bucket type.
#[derive(Debug)]
pub struct BucketCreator<'a> {
    bucket: &'a Bucket,
    config: &'a Config,
}

impl<'a> BucketCreator<'a> {
    pub fn new(config: &'a Config, bucket: &'a Bucket) -> Self {
        Self { bucket, config }
    }

    pub async fn create(&self) -> Result<(), RequestError> {
        let region = get_region(self.config.s3())?;
        let constraint = BucketLocationConstraint::from(region.as_str());

        let stack = self.config.stack().as_str();
        let bucket_name = self.bucket.name();
        let bucket_type = self.bucket.bucket_type().to_string();

        let cfg = CreateBucketConfiguration::builder()
            .location_constraint(constraint)
            .tags(
                Tag::builder()
                    .key(BUCKET_TAG_ORIGIN_KEY)
                    .value(BUCKET_TAG_ORIGIN_VAL)
                    .build()
                    .unwrap(),
            )
            .tags(
                Tag::builder()
                    .key(BUCKET_TAG_STACK_KEY)
                    .value(stack)
                    .build()
                    .unwrap(),
            )
            .tags(
                Tag::builder()
                    .key(BUCKET_TAG_TYPE_KEY)
                    .value(bucket_type)
                    .build()
                    .unwrap(),
            )
            .build();

        self.config
            .s3()
            .create_bucket()
            .create_bucket_configuration(cfg)
            .bucket(bucket_name)
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!("failed to create bucket {} ({:?})", bucket_name, e))
            })?;

        Ok(())
    }

    pub async fn rollback(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.config
            .s3()
            .delete_bucket()
            .bucket(bucket_name)
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!("failed to delete bucket {} ({:?})", bucket_name, e))
            })?;

        Ok(())
    }

    pub async fn setup(&self) -> Result<(), RequestError> {
        match self.bucket.bucket_type() {
            Type::Public => self.setup_public_bucket().await,
            Type::Replication => self.setup_replication_bucket().await,
            Type::Standard => self.setup_standard_bucket().await,
            _ => Err(RequestError::UnsupportedOperation(format!(
                "setup not supported for {} buckets",
                self.bucket.bucket_type()
            ))),
        }
    }

    async fn setup_public_bucket(&self) -> Result<(), RequestError> {
        self.add_deny_upload_policy().await?;
        self.enable_versioning().await?;
        self.add_lifecycle(STORAGE_CLASS_PUBLIC).await?;
        self.enable_notifications().await?;
        self.enable_bucket_logging().await?;
        self.enable_inventory().await?;
        self.remove_deny_upload_policy().await?;
        self.enable_public_access().await?;
        self.add_public_access_policy().await
    }

    async fn setup_replication_bucket(&self) -> Result<(), RequestError> {
        self.enable_versioning().await?;
        self.add_lifecycle(STORAGE_CLASS_REPLICATION).await
    }

    async fn setup_standard_bucket(&self) -> Result<(), RequestError> {
        self.add_deny_upload_policy().await?;
        self.enable_versioning().await?;
        self.add_lifecycle(STORAGE_CLASS_STANDARD).await?;
        self.enable_notifications().await?;
        self.enable_bucket_logging().await?;
        self.enable_inventory().await?;
        self.remove_deny_upload_policy().await
    }

    async fn add_deny_upload_policy(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        let policy = serde_json::json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Sid": "DenyAllUploads",
                "Effect": "Deny",
                "Principal": "*",
                "Action": "s3:PutObject",
                "Resource": format!("arn:aws:s3:::{}/*", bucket_name)
            }]
        });

        self.config
            .s3()
            .put_bucket_policy()
            .bucket(bucket_name)
            .policy(policy.to_string())
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to apply deny upload policy to {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }

    async fn add_lifecycle(
        &self,
        transition_class: TransitionStorageClass,
    ) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        let expiration = LifecycleRule::builder()
            .id("ExpireOldVersions")
            .status(ExpirationStatus::Enabled)
            .filter(LifecycleRuleFilter::builder().prefix("").build())
            .abort_incomplete_multipart_upload(
                AbortIncompleteMultipartUpload::builder()
                    .days_after_initiation(EXPIRE_ABORTED_MULTIPART_DAYS as i32)
                    .build(),
            )
            .expiration(
                LifecycleExpiration::builder()
                    .expired_object_delete_marker(true)
                    .build(),
            )
            .noncurrent_version_expiration(
                NoncurrentVersionExpiration::builder()
                    .noncurrent_days(EXPIRE_NONCURRENT_VERSION_DAYS as i32)
                    .build(),
            )
            .build()
            .map_err(|e| {
                RequestError::S3Error(format!("failed to build expiration rule: {:?}", e))
            })?;

        let transition = LifecycleRule::builder()
            .id(transition_class.as_str())
            .status(ExpirationStatus::Enabled)
            .filter(LifecycleRuleFilter::builder().prefix("").build())
            .transitions(
                Transition::builder()
                    .days(STORAGE_TRANSITION_DAYS as i32)
                    .storage_class(transition_class)
                    .build(),
            )
            .build()
            .map_err(|e| {
                RequestError::S3Error(format!("failed to build transition rule: {:?}", e))
            })?;

        let rules = vec![expiration, transition];

        self.config
            .s3()
            .put_bucket_lifecycle_configuration()
            .bucket(bucket_name)
            .lifecycle_configuration(
                BucketLifecycleConfiguration::builder()
                    .set_rules(Some(rules))
                    .build()
                    .map_err(|e| {
                        RequestError::S3Error(format!(
                            "failed to build lifecycle configuration: {:?}",
                            e
                        ))
                    })?,
            )
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to apply lifecycle policy {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }

    async fn add_public_access_policy(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        let policy = serde_json::json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Sid": "AllowPublicRead",
                "Effect": "Allow",
                "Principal": "*",
                "Action": "s3:GetObject",
                "Resource": format!("arn:aws:s3:::{}/*", bucket_name)
            }]
        });

        self.config
            .s3()
            .put_bucket_policy()
            .bucket(bucket_name)
            .policy(policy.to_string())
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to apply public access policy to {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }

    async fn enable_bucket_logging(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();
        let dest_bucket = self.config.stack().managed_bucket();

        self.config
            .s3()
            .put_bucket_logging()
            .bucket(bucket_name)
            .bucket_logging_status(
                aws_sdk_s3::types::BucketLoggingStatus::builder()
                    .logging_enabled(
                        aws_sdk_s3::types::LoggingEnabled::builder()
                            .target_bucket(dest_bucket)
                            .target_prefix(format!("audit/{}/", bucket_name))
                            .build()
                            .map_err(|e| {
                                RequestError::S3Error(format!(
                                    "failed to build logging config: {:?}",
                                    e
                                ))
                            })?,
                    )
                    .build(),
            )
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to enable logging on {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }

    async fn enable_inventory(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();
        let dest_bucket = self.config.stack().managed_bucket();

        self.config
            .s3()
            .put_bucket_inventory_configuration()
            .bucket(bucket_name)
            .id(INVENTORY_ID)
            .inventory_configuration(
                InventoryConfiguration::builder()
                    .is_enabled(true)
                    .id(INVENTORY_ID)
                    .included_object_versions(InventoryIncludedObjectVersions::Current)
                    .schedule(
                        InventorySchedule::builder()
                            .frequency(InventoryFrequency::Daily)
                            .build()
                            .map_err(|e| {
                                RequestError::S3Error(format!(
                                    "failed to build inventory schedule: {:?}",
                                    e
                                ))
                            })?,
                    )
                    .destination(
                        InventoryDestination::builder()
                            .s3_bucket_destination(
                                InventoryS3BucketDestination::builder()
                                    .account_id(self.config.account_id())
                                    .bucket(format!("arn:aws:s3:::{}", dest_bucket))
                                    .format(INVENTORY_FORMAT)
                                    .prefix(INVENTORY_PREFIX)
                                    .build()
                                    .map_err(|e| {
                                        RequestError::S3Error(format!(
                                            "failed to build inventory destination: {:?}",
                                            e
                                        ))
                                    })?,
                            )
                            .build(),
                    )
                    .optional_fields(InventoryOptionalField::Size)
                    .optional_fields(InventoryOptionalField::LastModifiedDate)
                    .optional_fields(InventoryOptionalField::StorageClass)
                    .optional_fields(InventoryOptionalField::ReplicationStatus)
                    .build()
                    .map_err(|e| {
                        RequestError::S3Error(format!(
                            "failed to build inventory configuration: {:?}",
                            e
                        ))
                    })?,
            )
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to enable inventory on {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }

    async fn enable_notifications(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.config
            .s3()
            .put_bucket_notification_configuration()
            .bucket(bucket_name)
            .notification_configuration(
                NotificationConfiguration::builder()
                    .event_bridge_configuration(EventBridgeConfiguration::builder().build())
                    .build(),
            )
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to enable notifications on {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }

    async fn enable_public_access(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.config
            .s3()
            .put_public_access_block()
            .bucket(bucket_name)
            .public_access_block_configuration(
                aws_sdk_s3::types::PublicAccessBlockConfiguration::builder()
                    .block_public_acls(false)
                    .ignore_public_acls(false)
                    .block_public_policy(false)
                    .restrict_public_buckets(false)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to enable public access on {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }

    pub async fn enable_replication(&self, replication: &Bucket) -> Result<(), RequestError> {
        let src_bucket_name = self.bucket.name();
        let repl_bucket_name = replication.name();

        self.config
            .s3()
            .put_bucket_replication()
            .bucket(src_bucket_name)
            .replication_configuration(
                ReplicationConfiguration::builder()
                    .role(self.config.replication_role_arn())
                    .rules(
                        ReplicationRule::builder()
                            .id("ReplicateAll")
                            .status(ReplicationRuleStatus::Enabled)
                            .priority(1)
                            .filter(ReplicationRuleFilter::builder().prefix("").build())
                            .destination(
                                Destination::builder()
                                    .bucket(format!("arn:aws:s3:::{}", repl_bucket_name))
                                    .replication_time(
                                        ReplicationTime::builder()
                                            .status(ReplicationTimeStatus::Enabled)
                                            .time(
                                                ReplicationTimeValue::builder().minutes(15).build(),
                                            )
                                            .build()
                                            .map_err(|e| {
                                                RequestError::S3Error(format!(
                                                    "failed to build replication time: {:?}",
                                                    e
                                                ))
                                            })?,
                                    )
                                    .metrics(
                                        Metrics::builder()
                                            .status(MetricsStatus::Enabled)
                                            .event_threshold(
                                                ReplicationTimeValue::builder().minutes(15).build(),
                                            )
                                            .build()
                                            .map_err(|e| {
                                                RequestError::S3Error(format!(
                                                    "failed to build replication metrics: {:?}",
                                                    e
                                                ))
                                            })?,
                                    )
                                    .build()
                                    .map_err(|e| {
                                        RequestError::S3Error(format!(
                                            "failed to build replication destination: {:?}",
                                            e
                                        ))
                                    })?,
                            )
                            .delete_marker_replication(
                                DeleteMarkerReplication::builder()
                                    .status(DeleteMarkerReplicationStatus::Enabled)
                                    .build(),
                            )
                            .build()
                            .map_err(|e| {
                                RequestError::S3Error(format!(
                                    "failed to build replication rule: {:?}",
                                    e
                                ))
                            })?,
                    )
                    .build()
                    .map_err(|e| {
                        RequestError::S3Error(format!(
                            "failed to build replication configuration: {:?}",
                            e
                        ))
                    })?,
            )
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to enable replication from {} to {} ({:?})",
                    src_bucket_name, repl_bucket_name, e
                ))
            })?;

        Ok(())
    }

    async fn enable_versioning(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.config
            .s3()
            .put_bucket_versioning()
            .bucket(bucket_name)
            .versioning_configuration(
                VersioningConfiguration::builder()
                    .status(BucketVersioningStatus::Enabled)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to enable versioning on {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }

    async fn remove_deny_upload_policy(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.config
            .s3()
            .delete_bucket_policy()
            .bucket(bucket_name)
            .send()
            .await
            .map_err(|e| {
                RequestError::S3Error(format!(
                    "failed to remove deny upload policy from {} ({:?})",
                    bucket_name, e
                ))
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_client::{TestClientBuilder, test_config_with_client};

    #[tokio::test]
    async fn test_setup_unsupported_for_internal_bucket() {
        let client = TestClientBuilder::new().build();
        let config = test_config_with_client(client);
        let bucket = Bucket::new("test-internal", Type::Internal).unwrap();
        let creator = BucketCreator::new(&config, &bucket);

        let result = creator.setup().await;

        assert!(result.is_err());
        match result.unwrap_err() {
            RequestError::UnsupportedOperation(msg) => {
                assert!(msg.contains("setup not supported for internal buckets"))
            }
            e => panic!("Expected UnsupportedOperation, got {:?}", e),
        }
    }
}
