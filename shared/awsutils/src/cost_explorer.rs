use aws_sdk_costexplorer::types::{
    DateInterval, Dimension, DimensionValues, Expression, Granularity, TagValues,
};
use chrono::{Datelike, NaiveDate, Utc};

use crate::errors::{RequestError, S3ResultExt};

const STACK_TAG_KEY: &str = "Stack";
const S3_SERVICE_NAME: &str = "Amazon Simple Storage Service";
const S3_TRANSFER_OUT_USAGE_GROUP: &str = "S3: Data Transfer - Internet (Out)";
const USAGE_QUANTITY_METRIC: &str = "UsageQuantity";

#[derive(Debug, Clone)]
pub struct DataTransferOut {
    pub bytes: u64,
    pub period_start: String,
    pub period_end: String,
}

/// Sum S3 internet egress (bytes) for the current calendar year so far,
/// scoped to a stack via the `Stack` cost allocation tag.
///
/// Window is `[Jan 1 of current year, today)` — Cost Explorer's end date is
/// exclusive, which conveniently excludes today's still-settling rows.
///
/// Returns `Ok(None)` when Cost Explorer reports no usage. The most likely
/// causes are: the `Stack` cost allocation tag has not been activated by the
/// org's payer account, the stack has had no internet egress this year, or
/// the Lambda is running on January 1 (empty window).
pub async fn s3_data_transfer_out_ytd(
    client: &aws_sdk_costexplorer::Client,
    stack: &str,
) -> Result<Option<DataTransferOut>, RequestError> {
    let (start, end) = year_to_date_utc();
    if start == end {
        return Ok(None);
    }

    let time_period = DateInterval::builder()
        .start(&start)
        .end(&end)
        .build()
        .s3_err("invalid Cost Explorer time period")?;

    let filter = Expression::builder()
        .and(service_is_s3())
        .and(usage_type_group_is_transfer_out())
        .and(stack_tag_is(stack))
        .build();

    let response = client
        .get_cost_and_usage()
        .time_period(time_period)
        .granularity(Granularity::Monthly)
        .metrics(USAGE_QUANTITY_METRIC)
        .filter(filter)
        .send()
        .await
        .s3_err("failed to query Cost Explorer")?;

    let amount_gb: f64 = response
        .results_by_time()
        .iter()
        .filter_map(|result| result.total().and_then(|t| t.get(USAGE_QUANTITY_METRIC)))
        .filter_map(|metric| metric.amount().and_then(|s| s.parse::<f64>().ok()))
        .sum();

    if amount_gb <= 0.0 {
        return Ok(None);
    }

    Ok(Some(DataTransferOut {
        bytes: (amount_gb * 1_000_000_000.0).round() as u64,
        period_start: start,
        period_end: end,
    }))
}

fn service_is_s3() -> Expression {
    Expression::builder()
        .dimensions(
            DimensionValues::builder()
                .key(Dimension::Service)
                .values(S3_SERVICE_NAME)
                .build(),
        )
        .build()
}

fn stack_tag_is(stack: &str) -> Expression {
    Expression::builder()
        .tags(
            TagValues::builder()
                .key(STACK_TAG_KEY)
                .values(stack)
                .build(),
        )
        .build()
}

fn usage_type_group_is_transfer_out() -> Expression {
    Expression::builder()
        .dimensions(
            DimensionValues::builder()
                .key(Dimension::UsageTypeGroup)
                .values(S3_TRANSFER_OUT_USAGE_GROUP)
                .build(),
        )
        .build()
}

fn year_to_date_utc() -> (String, String) {
    let today = Utc::now().date_naive();
    let jan_first = NaiveDate::from_ymd_opt(today.year(), 1, 1)
        .expect("january 1 of current year is always a valid date");
    (
        jan_first.format("%Y-%m-%d").to_string(),
        today.format("%Y-%m-%d").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_year_to_date_window_starts_jan_first_of_current_year() {
        let (start, end) = year_to_date_utc();
        let today = Utc::now().date_naive();
        assert_eq!(start, format!("{}-01-01", today.year()));
        assert_eq!(end, today.format("%Y-%m-%d").to_string());
    }
}
