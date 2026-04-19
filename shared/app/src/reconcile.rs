use std::collections::HashMap;

use awsutils::{
    bucket::Bucket,
    bucket_creator::BucketCreatorParams,
    bucket_reconciliator::{BucketReconciliator, ReconcileReport},
};
use base::bucket::Type;
use constants::REPLICATION_SUFFIX;

use crate::config::Config;

/// Reconcile every bucket in a stack, pairing sources with their replication
/// buckets where possible. Returns reports sorted by bucket name.
pub async fn reconcile_stack(config: &Config, mut buckets: Vec<Bucket>) -> Vec<ReconcileReport> {
    buckets.sort_by(|a, b| a.name().cmp(b.name()));

    let mut source_buckets = Vec::new();
    let mut replication_buckets = Vec::new();
    for bucket in &buckets {
        match bucket.bucket_type() {
            Type::Standard | Type::Public => source_buckets.push(bucket),
            Type::Replication => replication_buckets.push(bucket),
            _ => source_buckets.push(bucket),
        }
    }

    let repl_map: HashMap<String, &Bucket> = replication_buckets
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

    let mut reports = Vec::new();

    for bucket in &source_buckets {
        let repl_bucket = repl_map.get(bucket.name()).copied();
        let reconciliator = BucketReconciliator::new(&params, bucket, repl_bucket);
        reports.push(reconciliator.reconcile().await);
    }

    for bucket in &replication_buckets {
        let reconciliator = BucketReconciliator::new(&params, bucket, None);
        reports.push(reconciliator.reconcile().await);
    }

    reports.sort_by(|a, b| a.bucket_name.cmp(&b.bucket_name));
    reports
}
