use awsutils::{
    bucket::{Bucket, Request, RequestConfig},
    file::File,
    BucketName,
};
use lambda_runtime::tracing;
use lambda_runtime::Error;

pub(crate) async fn perform(config: &RequestConfig, file: &File) -> Result<(), Error> {
    tracing::info!("Retrieving request file from S3");

    let names = match awsutils::bucket::get_request_names(&config.s3_client, &file).await {
        Ok(names) => names,
        Err(e) => {
            tracing::error!("Error getting bucket names: {}", e);
            // TODO: upload error report
            return Ok(());
        }
    };

    tracing::info!("Bucket names: {:?}", names);
    tracing::info!("Parsing bucket names");

    let buckets = match parse_bucket_names(config, &names) {
        Ok(buckets) => buckets,
        Err(e) => {
            tracing::error!("Error parsing bucket names: {}", e);
            // TODO: upload error report
            return Ok(());
        }
    };

    tracing::info!("Buckets to create: {:?}", buckets);
    tracing::info!("Creating buckets");

    // create_buckets(config, buckets);
    Ok(())
}

// TODO: move
fn parse_bucket_names(
    config: &RequestConfig,
    _: &Vec<String>,
) -> Result<Vec<(Bucket, Bucket)>, Error> {
    let mut buckets: Vec<(Bucket, Bucket)> = Vec::new();

    // let contents = s3::download(s3_client, file);
    let bucket = BucketName::new("test-1")?;
    let primary = Request::primary_bucket(&config.stack, &bucket)?;
    let replication = Request::replication_bucket(&config.stack, &bucket)?;

    // TODO: check whether bucket exists, skip if it does
    // bucket::exists(s3_client, primary)
    buckets.push((primary, replication));

    Ok(buckets)
}
