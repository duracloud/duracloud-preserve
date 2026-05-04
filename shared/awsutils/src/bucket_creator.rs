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

use constants::*;

use base::Stack;

use crate::bucket::{Bucket, RequestError, Type};
use crate::errors::S3ResultExt;
use crate::{bucket_policy, config};

pub const INVENTORY_FORMAT: InventoryFormat = InventoryFormat::Parquet;

pub const STORAGE_CLASS_STANDARD_DEFAULT: TransitionStorageClass =
    TransitionStorageClass::IntelligentTiering;
pub(crate) const STORAGE_CLASS_PUBLIC_DEFAULT: TransitionStorageClass =
    TransitionStorageClass::IntelligentTiering;
pub(crate) const STORAGE_CLASS_REPLICATION_DEFAULT: TransitionStorageClass =
    TransitionStorageClass::DeepArchive;

/// Handles bucket setup by delegating to the appropriate methods per bucket type.
#[derive(Debug)]
pub struct BucketCreator<'a> {
    account_id: &'a str,
    bucket: &'a Bucket,
    client: &'a aws_sdk_s3::Client,
    replication_role_arn: &'a str,
    stack: &'a Stack,
    storage_tier_override: Option<TransitionStorageClass>,
}

pub struct BucketCreatorParams<'a> {
    pub account_id: &'a str,
    pub client: &'a aws_sdk_s3::Client,
    pub replication_role_arn: &'a str,
    pub stack: &'a Stack,
}

impl<'a> BucketCreator<'a> {
    pub fn new(
        params: BucketCreatorParams<'a>,
        bucket: &'a Bucket,
        storage_tier_override: Option<TransitionStorageClass>,
    ) -> Self {
        let BucketCreatorParams {
            account_id,
            client,
            replication_role_arn,
            stack,
        } = params;

        Self {
            account_id,
            bucket,
            client,
            replication_role_arn,
            stack,
            storage_tier_override,
        }
    }

    fn storage_tier(&self) -> TransitionStorageClass {
        self.storage_tier_override
            .clone()
            .unwrap_or_else(|| default_storage_class(self.bucket.bucket_type()))
    }

    pub async fn create(&self) -> Result<(), RequestError> {
        let region = config::get_region(self.client)?;
        let constraint = BucketLocationConstraint::from(region.as_str());

        let stack = self.stack.as_str();
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
            .tags(
                Tag::builder()
                    .key(BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY)
                    .value(self.storage_tier().as_str())
                    .build()
                    .unwrap(),
            )
            .build();

        self.client
            .create_bucket()
            .create_bucket_configuration(cfg)
            .bucket(bucket_name)
            .send()
            .await
            .s3_err(format!("failed to create bucket {bucket_name}"))?;

        Ok(())
    }

    pub async fn rollback(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.client
            .delete_bucket()
            .bucket(bucket_name)
            .send()
            .await
            .s3_err(format!("failed to delete bucket {bucket_name}"))?;

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
        self.add_lifecycle(self.storage_tier()).await?;
        self.enable_notifications().await?;
        self.enable_bucket_logging().await?;
        self.enable_inventory().await?;
        self.remove_deny_upload_policy().await?;
        self.enable_public_access().await?;
        self.add_public_access_policy().await
    }

    async fn setup_replication_bucket(&self) -> Result<(), RequestError> {
        self.enable_versioning().await?;
        self.add_lifecycle(self.storage_tier()).await
    }

    async fn setup_standard_bucket(&self) -> Result<(), RequestError> {
        self.add_deny_upload_policy().await?;
        self.enable_versioning().await?;
        self.add_lifecycle(self.storage_tier()).await?;
        self.enable_notifications().await?;
        self.enable_bucket_logging().await?;
        self.enable_inventory().await?;
        self.remove_deny_upload_policy().await
    }

    async fn add_deny_upload_policy(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.client
            .put_bucket_policy()
            .bucket(bucket_name)
            .policy(bucket_policy::deny_uploads_policy(bucket_name))
            .send()
            .await
            .s3_err(format!(
                "failed to apply deny upload policy to {bucket_name}"
            ))?;

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
            .s3_err("failed to build expiration rule")?;

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
            .s3_err("failed to build transition rule")?;

        let legacy_duracloud = LifecycleRule::builder()
            .id("ExpireLegacyDuraCloudFiles")
            .status(ExpirationStatus::Enabled)
            .filter(
                LifecycleRuleFilter::builder()
                    .tag(
                        Tag::builder()
                            .key(LIFECYCLE_LEGACY_DURACLOUD_FILE_TAG_KEY)
                            .value(LIFECYCLE_LEGACY_DURACLOUD_FILE_TAG_VAL)
                            .build()
                            .s3_err("failed to build legacy duracloud tag")?,
                    )
                    .build(),
            )
            .expiration(
                LifecycleExpiration::builder()
                    .days(EXPIRE_LEGACY_DURACLOUD_FILE_DAYS as i32)
                    .build(),
            )
            .build()
            .s3_err("failed to build legacy duracloud expiration rule")?;

        let rules = vec![expiration, transition, legacy_duracloud];

        self.client
            .put_bucket_lifecycle_configuration()
            .bucket(bucket_name)
            .lifecycle_configuration(
                BucketLifecycleConfiguration::builder()
                    .set_rules(Some(rules))
                    .build()
                    .s3_err("failed to build lifecycle configuration")?,
            )
            .send()
            .await
            .s3_err(format!("failed to apply lifecycle policy {bucket_name}"))?;

        Ok(())
    }

    async fn add_public_access_policy(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.client
            .put_bucket_policy()
            .bucket(bucket_name)
            .policy(bucket_policy::public_read_policy(bucket_name))
            .send()
            .await
            .s3_err(format!(
                "failed to apply public access policy to {bucket_name}"
            ))?;

        Ok(())
    }

    async fn enable_bucket_logging(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();
        let logging = self.stack.logging_prefix_path(bucket_name);

        self.client
            .put_bucket_logging()
            .bucket(bucket_name)
            .bucket_logging_status(
                aws_sdk_s3::types::BucketLoggingStatus::builder()
                    .logging_enabled(
                        aws_sdk_s3::types::LoggingEnabled::builder()
                            .target_bucket(logging.bucket())
                            .target_prefix(logging.key())
                            .build()
                            .s3_err("failed to build logging config")?,
                    )
                    .build(),
            )
            .send()
            .await
            .s3_err(format!("failed to enable logging on {bucket_name}"))?;

        Ok(())
    }

    async fn enable_inventory(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();
        let dest_bucket = self.stack.managed_bucket();

        self.client
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
                            .s3_err("failed to build inventory schedule")?,
                    )
                    .destination(
                        InventoryDestination::builder()
                            .s3_bucket_destination(
                                InventoryS3BucketDestination::builder()
                                    .account_id(self.account_id)
                                    .bucket(format!("arn:aws:s3:::{}", dest_bucket))
                                    .format(INVENTORY_FORMAT)
                                    .prefix(MANIFESTS_PREFIX)
                                    .build()
                                    .s3_err("failed to build inventory destination")?,
                            )
                            .build(),
                    )
                    .optional_fields(InventoryOptionalField::Size)
                    .optional_fields(InventoryOptionalField::LastModifiedDate)
                    .optional_fields(InventoryOptionalField::StorageClass)
                    .optional_fields(InventoryOptionalField::ReplicationStatus)
                    .build()
                    .s3_err("failed to build inventory configuration")?,
            )
            .send()
            .await
            .s3_err(format!("failed to enable inventory on {bucket_name}"))?;

        Ok(())
    }

    async fn enable_notifications(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.client
            .put_bucket_notification_configuration()
            .bucket(bucket_name)
            .notification_configuration(
                NotificationConfiguration::builder()
                    .event_bridge_configuration(EventBridgeConfiguration::builder().build())
                    .build(),
            )
            .send()
            .await
            .s3_err(format!("failed to enable notifications on {bucket_name}"))?;

        Ok(())
    }

    async fn enable_public_access(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.client
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
            .s3_err(format!("failed to enable public access on {bucket_name}"))?;

        Ok(())
    }

    pub async fn enable_replication(&self, replication: &Bucket) -> Result<(), RequestError> {
        let src_bucket_name = self.bucket.name();
        let repl_bucket_name = replication.name();

        self.client
            .put_bucket_replication()
            .bucket(src_bucket_name)
            .replication_configuration(
                ReplicationConfiguration::builder()
                    .role(self.replication_role_arn)
                    .rules(
                        ReplicationRule::builder()
                            .id("ReplicateAll")
                            .status(ReplicationRuleStatus::Enabled)
                            .priority(1)
                            .filter(ReplicationRuleFilter::builder().prefix("").build())
                            .destination(
                                Destination::builder()
                                    .bucket(format!("arn:aws:s3:::{repl_bucket_name}"))
                                    .replication_time(
                                        ReplicationTime::builder()
                                            .status(ReplicationTimeStatus::Enabled)
                                            .time(
                                                ReplicationTimeValue::builder().minutes(15).build(),
                                            )
                                            .build()
                                            .s3_err("failed to build replication time")?,
                                    )
                                    .metrics(
                                        Metrics::builder()
                                            .status(MetricsStatus::Enabled)
                                            .event_threshold(
                                                ReplicationTimeValue::builder().minutes(15).build(),
                                            )
                                            .build()
                                            .s3_err("failed to build replication metrics")?,
                                    )
                                    .build()
                                    .s3_err("failed to build replication destination")?,
                            )
                            .delete_marker_replication(
                                DeleteMarkerReplication::builder()
                                    .status(DeleteMarkerReplicationStatus::Enabled)
                                    .build(),
                            )
                            .build()
                            .s3_err("failed to build replication rule")?,
                    )
                    .build()
                    .s3_err("failed to build replication configuration")?,
            )
            .send()
            .await
            .s3_err(format!(
                "failed to enable replication from {src_bucket_name} to {repl_bucket_name}"
            ))?;

        Ok(())
    }

    async fn enable_versioning(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.client
            .put_bucket_versioning()
            .bucket(bucket_name)
            .versioning_configuration(
                VersioningConfiguration::builder()
                    .status(BucketVersioningStatus::Enabled)
                    .build(),
            )
            .send()
            .await
            .s3_err(format!("failed to enable versioning on {bucket_name}"))?;

        Ok(())
    }

    async fn remove_deny_upload_policy(&self) -> Result<(), RequestError> {
        let bucket_name = self.bucket.name();

        self.client
            .delete_bucket_policy()
            .bucket(bucket_name)
            .send()
            .await
            .s3_err(format!(
                "failed to remove deny upload policy from {bucket_name}"
            ))?;

        Ok(())
    }
}

/// Resolve the default transition storage class for a bucket type.
pub fn default_storage_class(bucket_type: &Type) -> TransitionStorageClass {
    match bucket_type {
        Type::Public => STORAGE_CLASS_PUBLIC_DEFAULT,
        Type::Replication => STORAGE_CLASS_REPLICATION_DEFAULT,
        Type::Standard => STORAGE_CLASS_STANDARD_DEFAULT,
        _ => STORAGE_CLASS_STANDARD_DEFAULT,
    }
}

#[cfg(test)]
mod tests {
    use base::Stack;

    use super::*;
    use test_support::TestClientBuilder;

    #[tokio::test]
    async fn test_setup_unsupported_for_internal_bucket() {
        let client = TestClientBuilder::new().build();
        let stack = Stack::new("test-stack").unwrap();
        let bucket = Bucket::new("test-internal", Type::Internal).unwrap();
        let creator = BucketCreator::new(
            BucketCreatorParams {
                account_id: "123456789",
                client: &client,
                replication_role_arn: "arn:aws:iam::123456789:role/test-replication-role",
                stack: &stack,
            },
            &bucket,
            None,
        );

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
