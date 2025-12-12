use awsutils::{bucket::RequestConfig, file::File};
use lambda_runtime::{tracing, Error};

pub(crate) async fn perform(config: &RequestConfig, file: &File) -> Result<(), Error> {
    tracing::info!("Retrieving request file from S3");

    let names = match awsutils::bucket::get_bucket_names(&config.s3_client, &file).await {
        Ok(names) => names,
        Err(e) => {
            tracing::error!("Error getting bucket names: {}", e);
            // TODO: upload error report
            return Ok(()); // User or transitory error. Try again.
        }
    };

    tracing::info!("Bucket names: {:?}", names);
    tracing::info!("Parsing bucket names");

    let buckets = match awsutils::bucket::review_bucket_names(config, &names) {
        Ok(buckets) => buckets,
        Err(e) => {
            tracing::error!("Error parsing bucket names: {}", e);
            // TODO: upload error report
            return Ok(()); // User error. Try again.
        }
    };

    tracing::info!("Buckets to create: {:?}", buckets);
    tracing::info!("Creating buckets");

    let issues = awsutils::bucket::create_buckets(config, &buckets).await;
    if issues.len() > 0 {
        // TODO: upload the issues
        tracing::error!("{:?}", issues);
        // Unexpected error, propagate for alert
        return Err(Error::from("Failed to create one or more buckets"));
    }

    tracing::info!("Perform complete");
    Ok(())
}
