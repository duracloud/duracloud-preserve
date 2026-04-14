use app::{config::Config, perform::checksum_report};
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use awsutils::{
    batch::S3BatchJobDetail,
    file::{self, File},
};
use base::stack::DateCtx;
use lambda_runtime::{Error, LambdaEvent, tracing};

pub(crate) async fn function_handler(
    config: &Config,
    perform_opts: &checksum_report::PerformOptions,
    event: LambdaEvent<CloudWatchEvent<S3BatchJobDetail>>,
) -> Result<(), Error> {
    let detail = event.payload.detail.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "CloudWatch event detail is required",
        )
    })?;

    let job = &detail.service_event_details;
    let status = job.status.as_str();

    if status.eq_ignore_ascii_case("failed") {
        tracing::error!(
            job_id = %job.job_id,
            job_arn = %job.job_arn,
            failure_codes = ?job.failure_codes,
            status_change_reason = ?job.status_change_reason,
            "S3 batch job failed",
        );
        return Err(std::io::Error::other(format!("Batch job {} failed", job.job_id)).into());
    }

    tracing::info!(job_id = %job.job_id, "Batch job was completed successfully");

    if config.debug_handler {
        tracing::info!("Debug handler mode enabled, skipping perform function.");
        return Ok(());
    }

    let receipt_file = File::from(
        config
            .stack()
            .metadata_checksums_receipts_path(&job.job_id, DateCtx::Latest),
    );

    if !file::exists(config.s3(), &receipt_file).await {
        tracing::info!("Batch job does not belong to this stack");
        return Ok(());
    }

    let stats = checksum_report::perform(config, &receipt_file, perform_opts).await?;

    tracing::info!(
        total_objects = stats.total_objects(),
        matches = stats.matches,
        mismatches = stats.mismatches,
        missing_replica = stats.missing_replica,
        missing_source = stats.missing_source,
        failed_source = stats.failed_source,
        failed_replication = stats.failed_replication,
        "Checksum report processing complete",
    );

    if !stats.is_ok() {
        return Err(std::io::Error::other("Checksum report encountered an error").into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::config as app_config;
    use lambda_runtime::{Context, LambdaEvent};
    use test_support::TestClientBuilder;

    #[tokio::test]
    async fn test_event_handler_complete_status() {
        let json = include_str!("../events/complete.json");
        let cw_event: CloudWatchEvent<S3BatchJobDetail> =
            serde_json::from_str(json).expect("failed to parse json");
        let detail = cw_event.detail.as_ref().expect("detail required");
        assert_eq!(detail.service_event_details.status, "Complete");

        let event = LambdaEvent::new(cw_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        let opts = checksum_report::PerformOptions::default();
        function_handler(&config, &opts, event).await.unwrap();
    }

    #[tokio::test]
    async fn test_event_handler_failed_status_returns_error() {
        let json = include_str!("../events/failed.json");
        let cw_event: CloudWatchEvent<S3BatchJobDetail> =
            serde_json::from_str(json).expect("failed to parse json");
        let detail = cw_event.detail.as_ref().expect("detail required");
        assert_eq!(detail.service_event_details.status, "Failed");

        let event = LambdaEvent::new(cw_event, Context::default());
        let sdk_config = TestClientBuilder::new().ok().build_sdk_config();
        let config = app_config::Config::for_tests(sdk_config, true);
        let opts = checksum_report::PerformOptions::default();
        let err = function_handler(&config, &opts, event)
            .await
            .expect_err("Expected handler to return error for failed status");
        assert!(err.to_string().contains("failed"));
    }
}
