use aws_sdk_cloudwatch::types::{Dimension, DimensionFilter, Statistic};
use aws_smithy_types::DateTime;
use chrono::Utc;

use crate::errors::{RequestError, S3ResultExt};

const NAMESPACE: &str = "AWS/S3";
const PERIOD_SECS: i32 = 86400;
const LOOKBACK_SECS: i64 = 2 * 86400;

#[derive(Debug, Clone)]
pub struct BucketMetrics {
    pub total_objects: u64,
    pub total_bytes: u64,
}

/// Fetch the latest CloudWatch S3 storage metrics for a bucket. S3 reports
/// these once per day, so we look back 2 days and take the most recent point.
/// Bytes are summed across all reported StorageType dimensions.
pub async fn get_bucket_metrics(
    client: &aws_sdk_cloudwatch::Client,
    bucket: &str,
) -> Result<BucketMetrics, RequestError> {
    let storage_types = list_storage_types(client, bucket).await?;

    let mut total_bytes: u64 = 0;
    for storage_type in &storage_types {
        if let Some(value) = latest_metric(client, "BucketSizeBytes", bucket, storage_type).await? {
            total_bytes = total_bytes.saturating_add(value as u64);
        }
    }

    let total_objects = latest_metric(client, "NumberOfObjects", bucket, "AllStorageTypes")
        .await?
        .map(|v| v as u64)
        .unwrap_or(0);

    Ok(BucketMetrics {
        total_objects,
        total_bytes,
    })
}

/// List the StorageType dimension values reported for BucketSizeBytes on a bucket.
async fn list_storage_types(
    client: &aws_sdk_cloudwatch::Client,
    bucket: &str,
) -> Result<Vec<String>, RequestError> {
    let response = client
        .list_metrics()
        .namespace(NAMESPACE)
        .metric_name("BucketSizeBytes")
        .dimensions(
            DimensionFilter::builder()
                .name("BucketName")
                .value(bucket)
                .build(),
        )
        .send()
        .await
        .s3_err(format!("failed to list metrics for {bucket}"))?;

    let storage_types = response
        .metrics()
        .iter()
        .filter_map(|m| {
            m.dimensions().iter().find_map(|d| {
                (d.name() == Some("StorageType"))
                    .then(|| d.value().map(str::to_owned))
                    .flatten()
            })
        })
        .collect();

    Ok(storage_types)
}

/// Fetch the most recent daily Average for an S3 metric.
async fn latest_metric(
    client: &aws_sdk_cloudwatch::Client,
    metric: &str,
    bucket: &str,
    storage_type: &str,
) -> Result<Option<f64>, RequestError> {
    let now = Utc::now().timestamp();
    let start = DateTime::from_secs(now - LOOKBACK_SECS);
    let end = DateTime::from_secs(now);

    let response = client
        .get_metric_statistics()
        .namespace(NAMESPACE)
        .metric_name(metric)
        .dimensions(
            Dimension::builder()
                .name("BucketName")
                .value(bucket)
                .build(),
        )
        .dimensions(
            Dimension::builder()
                .name("StorageType")
                .value(storage_type)
                .build(),
        )
        .start_time(start)
        .end_time(end)
        .period(PERIOD_SECS)
        .statistics(Statistic::Average)
        .send()
        .await
        .s3_err(format!(
            "failed to get {metric} for {bucket}/{storage_type}"
        ))?;

    let latest = response
        .datapoints()
        .iter()
        .max_by_key(|d| d.timestamp().map(DateTime::secs).unwrap_or(0))
        .and_then(|d| d.average());

    Ok(latest)
}
