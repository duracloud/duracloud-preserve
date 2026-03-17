use std::collections::HashMap;

use apputils::bucket::{REPLICATION_SUFFIX, Type};
use awsutils::{
    bucket_creator::BucketCreatorParams,
    bucket_reconciliator::{BucketReconciliator, ReconcileReport},
};

use crate::{bucket, config::Config, errors::BucketReconciliationError};

pub struct PerformOptions {
    pub fail_on_drift: bool,
}

#[derive(Debug)]
pub struct PerformReport {
    pub bucket_reports: Vec<ReconcileReport>,
    pub processed: usize,
    pub ok: usize,
    pub drift: usize,
    pub errors: usize,
}

impl PerformReport {
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }

    pub fn has_drift(&self) -> bool {
        self.drift > 0
    }
}

/// Run bucket reconciliation for all bucket-request buckets in a stack.
/// For now this is a reporter-only: reads and compares configuration.
pub async fn perform(
    config: &Config,
    opts: &PerformOptions,
) -> Result<PerformReport, BucketReconciliationError> {
    let mut buckets = bucket::get_requested(config.s3(), config.stack()).await?;

    buckets.sort_by(|a, b| a.name().cmp(b.name()));

    // Split into source (Standard/Public) and replication buckets.
    let mut source_buckets = Vec::new();
    let mut replication_buckets = Vec::new();

    for bucket in &buckets {
        match bucket.bucket_type() {
            Type::Standard | Type::Public => source_buckets.push(bucket),
            Type::Replication => replication_buckets.push(bucket),
            _ => source_buckets.push(bucket), // will get an error in reconciliation
        }
    }

    // Replication buckets have names like "{source_name}-repl".
    let repl_map: HashMap<String, &_> = replication_buckets
        .iter()
        .filter_map(|b| {
            b.name()
                .strip_suffix(REPLICATION_SUFFIX)
                .map(|base| (base.to_string(), *b))
        })
        .collect();

    let params = BucketCreatorParams {
        account_id: config.account_id(),
        client: config.s3(),
        replication_role_arn: config.replication_role_arn(),
        stack: config.stack(),
    };

    let mut bucket_reports = Vec::new();

    // Reconcile source buckets (with replication pairing).
    for bucket in &source_buckets {
        let repl_bucket = repl_map.get(bucket.name()).copied();
        let reconciliator = BucketReconciliator::new(&params, bucket, repl_bucket);
        let report = reconciliator.reconcile().await;
        bucket_reports.push(report);
    }

    // Reconcile replication buckets (no replication-config step).
    for bucket in &replication_buckets {
        let reconciliator = BucketReconciliator::new(&params, bucket, None);
        let report = reconciliator.reconcile().await;
        bucket_reports.push(report);
    }

    // Sort reports by bucket name for stable output.
    bucket_reports.sort_by(|a, b| a.bucket_name.cmp(&b.bucket_name));

    // Aggregate counts.
    let processed = bucket_reports.len();
    let mut ok = 0;
    let mut drift = 0;
    let mut errors = 0;

    for report in &bucket_reports {
        if report.has_errors() {
            errors += 1;
        } else if report.has_drift() {
            drift += 1;
        } else {
            ok += 1;
        }
    }

    let perform_report = PerformReport {
        bucket_reports,
        processed,
        ok,
        drift,
        errors,
    };

    if opts.fail_on_drift && perform_report.has_drift() {
        return Err(BucketReconciliationError::DriftDetected(format!(
            "{} bucket(s) have configuration drift",
            perform_report.drift
        )));
    }

    Ok(perform_report)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config as app_config;
    use test_support::TestClientBuilder;

    const TEST_STACK: &str = "test-stack";
    const TEST_ACCOUNT_ID: &str = "123456789";
    const TEST_REPL_ROLE_ARN: &str = "arn:aws:iam::123456789:role/test-replication-role";

    fn list_buckets_xml(names: &[&str]) -> String {
        let buckets = names
            .iter()
            .map(|name| {
                format!(
                    "<Bucket><Name>{name}</Name><CreationDate>2025-01-01T00:00:00.000Z</CreationDate></Bucket>"
                )
            })
            .collect::<Vec<_>>()
            .join("");

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Owner><ID>owner-id</ID><DisplayName>owner</DisplayName></Owner>
  <Buckets>{buckets}</Buckets>
</ListAllMyBucketsResult>"#
        )
    }

    fn tagging_xml(tags: &[(&str, &str)]) -> String {
        let entries = tags
            .iter()
            .map(|(k, v)| format!("<Tag><Key>{k}</Key><Value>{v}</Value></Tag>"))
            .collect::<Vec<_>>()
            .join("");

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet>{entries}</TagSet>
</Tagging>"#
        )
    }

    fn bucket_request_tags(bucket_type: &str) -> String {
        tagging_xml(&[
            ("Stack", TEST_STACK),
            ("BucketType", bucket_type),
            ("BucketOrigin", "bucket-request"),
        ])
    }

    fn reconcile_tagging_xml(transition_class: &str) -> String {
        tagging_xml(&[("TransitionStorageClass", transition_class)])
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
    <AbortIncompleteMultipartUpload><DaysAfterInitiation>3</DaysAfterInitiation></AbortIncompleteMultipartUpload>
    <NoncurrentVersionExpiration><NoncurrentDays>14</NoncurrentDays></NoncurrentVersionExpiration>
    <Expiration><ExpiredObjectDeleteMarker>true</ExpiredObjectDeleteMarker></Expiration>
  </Rule>
  <Rule>
    <ID>{transition_class}</ID>
    <Status>Enabled</Status>
    <Filter><Prefix></Prefix></Filter>
    <Transition>
      <Days>7</Days>
      <StorageClass>{transition_class}</StorageClass>
    </Transition>
  </Rule>
</LifecycleConfiguration>"#
        )
    }

    fn replication_xml(dest_bucket: &str, role_arn: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ReplicationConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Role>{role_arn}</Role>
  <Rule>
    <ID>ReplicateAll</ID>
    <Status>Enabled</Status>
    <Priority>1</Priority>
    <Filter><Prefix></Prefix></Filter>
    <Destination>
      <Bucket>arn:aws:s3:::{dest_bucket}</Bucket>
      <ReplicationTime>
        <Status>Enabled</Status>
        <Time><Minutes>15</Minutes></Time>
      </ReplicationTime>
      <Metrics>
        <Status>Enabled</Status>
        <EventThreshold><Minutes>15</Minutes></EventThreshold>
      </Metrics>
    </Destination>
    <DeleteMarkerReplication><Status>Enabled</Status></DeleteMarkerReplication>
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

    fn inventory_xml(managed_bucket_arn: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<InventoryConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Id>inventory</Id>
  <IsEnabled>true</IsEnabled>
  <IncludedObjectVersions>Current</IncludedObjectVersions>
  <Schedule><Frequency>Daily</Frequency></Schedule>
  <Destination>
    <S3BucketDestination>
      <AccountId>{TEST_ACCOUNT_ID}</AccountId>
      <Bucket>{managed_bucket_arn}</Bucket>
      <Format>Parquet</Format>
      <Prefix>manifests</Prefix>
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

    fn add_standard_reconcile_responses(
        builder: TestClientBuilder,
        bucket_name: &str,
        repl_name: Option<&str>,
        versioning_status: &str,
        managed_bucket: &str,
    ) -> TestClientBuilder {
        let class = "GLACIER_IR";
        let builder = builder
            // transition tag
            .success(reconcile_tagging_xml(class), None)
            // versioning
            .success(versioning_xml(versioning_status), None)
            // lifecycle
            .success(lifecycle_xml_full(class), None);

        let builder = if let Some(repl_name) = repl_name {
            builder.success(replication_xml(repl_name, TEST_REPL_ROLE_ARN), None)
        } else {
            builder
        };

        builder
            // notifications
            .success(notification_xml_with_eventbridge(), None)
            // logging
            .success(
                logging_xml(managed_bucket, &format!("audit/{bucket_name}/")),
                None,
            )
            // inventory
            .success(
                inventory_xml(&format!("arn:aws:s3:::{managed_bucket}")),
                None,
            )
    }

    fn add_replication_bucket_reconcile_responses(
        builder: TestClientBuilder,
        _bucket_name: &str,
    ) -> TestClientBuilder {
        let class = "DEEP_ARCHIVE";
        builder
            // transition tag
            .success(reconcile_tagging_xml(class), None)
            // versioning
            .success(versioning_xml("Enabled"), None)
            // lifecycle
            .success(lifecycle_xml_full(class), None)
    }

    fn test_opts() -> PerformOptions {
        PerformOptions {
            fail_on_drift: false,
        }
    }

    #[tokio::test]
    async fn test_perform_aggregates_ok_drift_and_error_counts() {
        let alpha = "test-stack-alpha";
        let alpha_repl = "test-stack-alpha-repl";
        let beta = "test-stack-beta";

        let builder = TestClientBuilder::new()
            // List + per-bucket tags (order matches list order)
            .success(list_buckets_xml(&[alpha, alpha_repl, beta]), None)
            .success(bucket_request_tags("standard"), None)
            .success(bucket_request_tags("replication"), None)
            .success(bucket_request_tags("standard"), None);

        let managed_bucket = format!("{TEST_STACK}-managed");

        let builder = add_standard_reconcile_responses(
            builder,
            alpha,
            Some(alpha_repl),
            "Suspended",
            &managed_bucket,
        );
        let builder =
            add_standard_reconcile_responses(builder, beta, None, "Enabled", &managed_bucket);
        let builder = add_replication_bucket_reconcile_responses(builder, alpha_repl);

        let sdk_config = builder.build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let report = perform(&config, &test_opts())
            .await
            .expect("perform should return report even with drift/error");

        assert_eq!(report.processed, 3);
        assert_eq!(report.ok, 1); // alpha-repl
        assert_eq!(report.drift, 1); // alpha (versioning drift)
        assert_eq!(report.errors, 1); // beta (missing repl pair -> replication step error)
        assert!(report.has_drift());
        assert!(report.has_errors());

        // Reports are sorted by bucket name.
        let names: Vec<&str> = report
            .bucket_reports
            .iter()
            .map(|r| r.bucket_name.as_str())
            .collect();
        assert_eq!(names, vec![alpha, alpha_repl, beta]);
    }

    #[tokio::test]
    async fn test_perform_best_effort_pairing_missing_replication_does_not_crash() {
        let source = "test-stack-alpha";

        let builder = TestClientBuilder::new()
            .success(list_buckets_xml(&[source]), None)
            .success(bucket_request_tags("standard"), None);

        let managed_bucket = format!("{TEST_STACK}-managed");
        let builder =
            add_standard_reconcile_responses(builder, source, None, "Enabled", &managed_bucket);

        let sdk_config = builder.build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let report = perform(&config, &test_opts())
            .await
            .expect("missing replication pair should be reported, not crash perform()");

        assert_eq!(report.processed, 1);
        assert_eq!(report.ok, 0);
        assert_eq!(report.drift, 0);
        assert_eq!(report.errors, 1);
        assert!(report.has_errors());

        let bucket_report = &report.bucket_reports[0];
        assert_eq!(bucket_report.bucket_name, source);
        let repl_step = bucket_report
            .steps
            .iter()
            .find(|s| s.name == "replication")
            .expect("replication step should exist for source bucket");
        assert!(matches!(
            repl_step.status,
            awsutils::bucket_reconciliator::StepStatus::Error(_)
        ));
    }

    #[tokio::test]
    async fn test_perform_empty_stack_returns_empty_report() {
        let sdk_config = TestClientBuilder::new()
            .success(list_buckets_xml(&[]), None)
            .build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);

        let report = perform(&config, &test_opts())
            .await
            .expect("empty stack should not error");

        assert_eq!(report.processed, 0);
        assert_eq!(report.ok, 0);
        assert_eq!(report.drift, 0);
        assert_eq!(report.errors, 0);
        assert!(report.bucket_reports.is_empty());
        assert!(!report.has_drift());
        assert!(!report.has_errors());
    }

    #[tokio::test]
    async fn test_perform_fail_on_drift_returns_error() {
        let source = "test-stack-alpha";
        let repl = "test-stack-alpha-repl";

        let builder = TestClientBuilder::new()
            .success(list_buckets_xml(&[source, repl]), None)
            .success(bucket_request_tags("standard"), None)
            .success(bucket_request_tags("replication"), None);

        let managed_bucket = format!("{TEST_STACK}-managed");

        let builder = add_standard_reconcile_responses(
            builder,
            source,
            Some(repl),
            "Suspended",
            &managed_bucket,
        );
        let builder = add_replication_bucket_reconcile_responses(builder, repl);

        let sdk_config = builder.build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, false);
        let opts = PerformOptions {
            fail_on_drift: true,
        };

        let err = perform(&config, &opts)
            .await
            .expect_err("fail_on_drift should return error when drift exists");

        match err {
            BucketReconciliationError::DriftDetected(msg) => {
                assert!(msg.contains("1 bucket(s) have configuration drift"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
