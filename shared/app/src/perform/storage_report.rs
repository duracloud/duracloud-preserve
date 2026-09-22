use awsutils::cost_explorer;
use base::{
    bucket::Type,
    stack::DateCtx,
    storage::{BillingTransferOut, StorageReport},
};
use bytes::Bytes;
use constants::{APPLICATION_JSON, TEXT_HTML};

use crate::{bucket, config::Config, errors::StorageReportError, upload};

#[derive(Debug, Clone, Copy, Default)]
pub struct PerformArgs {
    pub storage_capacity_bytes: Option<u64>,
}

pub async fn perform(
    config: &Config,
    args: &PerformArgs,
) -> Result<StorageReport, StorageReportError> {
    let owner = awsutils::config::get_account_name(&config.clients().account)
        .await
        .map_err(StorageReportError::AccountInformation)?;

    let buckets = bucket::list_for_stack_by_type(
        config.s3(),
        config.stack(),
        &[Type::Public, Type::Standard],
    )
    .await
    .map_err(StorageReportError::BucketDiscovery)?;

    let bucket_stats = bucket::fetch_latest_inventory_stats(config, buckets).await?;

    let data_transfer_out = match cost_explorer::s3_data_transfer_out_ytd(
        &config.clients().cost_explorer,
        config.stack().as_str(),
    )
    .await
    {
        Ok(value) => value.map(|t| BillingTransferOut {
            bytes: t.bytes,
            period_start: t.period_start,
            period_end: t.period_end,
        }),
        Err(err) => {
            tracing::warn!(
                ?err,
                "Cost Explorer query failed; storage report will omit transfer-out metric"
            );
            None
        }
    };

    let storage_report = StorageReport::assemble(
        owner,
        config.stack().as_str().to_string(),
        args.storage_capacity_bytes,
        bucket_stats,
        data_transfer_out,
    );

    let stats_bytes = Bytes::from(serde_json::to_vec(&storage_report)?);
    let html_bytes = Bytes::from(storage_report.to_html()?);

    upload::put_versioned_bytes(
        config,
        DateCtx::Today,
        html_bytes,
        TEXT_HTML,
        |ctx| config.stack().reports_storage_path(ctx),
        StorageReportError::UploadError,
    )
    .await?;

    upload::put_versioned_bytes(
        config,
        DateCtx::Today,
        stats_bytes,
        APPLICATION_JSON,
        |ctx| config.stack().metadata_storage_stats_path(ctx),
        StorageReportError::UploadError,
    )
    .await?;

    Ok(storage_report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use test_support::{TestClientBuilder, recorded_requests, replay_event_with_content_type};

    #[tokio::test]
    async fn test_perform_uses_account_name_in_report_and_uploads() {
        let builder = TestClientBuilder::new()
            .success(
                r#"{"AccountName":"Example Owner"}"#,
                Some("application/x-amz-json-1.1".to_string()),
            )
            .success(
                r#"<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Buckets/></ListAllMyBucketsResult>"#,
                Some("application/xml".to_string()),
            );
        // Cost Explorer skips its request on January 1 (empty year-to-date window).
        let builder = if chrono::Utc::now().ordinal() == 1 {
            builder
        } else {
            builder.success(
                r#"{"ResultsByTime":[]}"#,
                Some("application/x-amz-json-1.1".to_string()),
            )
        };
        let copy_result = r#"<CopyObjectResult><ETag>"etag"</ETag></CopyObjectResult>"#;
        let (sdk_config, replay) = builder
            .ok()
            .success(copy_result, None)
            .ok()
            .success(copy_result, None)
            .build_sdk_config_with_replay();
        let config = Config::for_tests(sdk_config, false);

        let report = perform(&config, &PerformArgs::default())
            .await
            .expect("storage report should succeed");

        assert_eq!(report.header.owner, "Example Owner");
        let requests = recorded_requests(&replay);
        assert!(requests[0].uri.contains("account."));
        let uploads: Vec<_> = requests
            .iter()
            .filter(|r| r.method == "PUT" && r.copy_source.is_none())
            .collect();
        assert_eq!(uploads.len(), 2);
        let html = uploads
            .iter()
            .find(|r| r.content_type.as_deref() == Some(TEXT_HTML))
            .unwrap();
        assert!(String::from_utf8_lossy(&html.body).contains("Example Owner"));
        let stats = uploads
            .iter()
            .find(|r| r.content_type.as_deref() == Some(APPLICATION_JSON))
            .unwrap();
        let stats: serde_json::Value = serde_json::from_slice(&stats.body).unwrap();
        assert_eq!(stats["owner"], "Example Owner");
    }

    #[tokio::test]
    async fn test_account_lookup_failure_stops_report_before_other_requests() {
        let (sdk_config, replay) = TestClientBuilder::new()
            .event(replay_event_with_content_type(
                "https://account.us-east-1.amazonaws.com/",
                403,
                r#"{"__type":"AccessDeniedException","message":"not authorized"}"#,
                Some("application/x-amz-json-1.1"),
            ))
            .build_sdk_config_with_replay();
        let config = Config::for_tests(sdk_config, false);

        let error = perform(&config, &PerformArgs::default()).await.unwrap_err();

        assert!(matches!(error, StorageReportError::AccountInformation(_)));
        assert_eq!(recorded_requests(&replay).len(), 1);
    }
}
