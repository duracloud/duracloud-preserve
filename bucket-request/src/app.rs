use awsutils::{bucket::RequestConfig, file::File};
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

    let buckets = match awsutils::bucket::review_bucket_names(config, &names) {
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
    // TODO: check whether bucket exists, skip if it does (see examples)
    // let creator = BucketCreator::new(config);
    // creator.create(primary)? // create
    // creator.setup(primary)? // deny + versioning + etc.
    // creator.create(replication)?
    // creator.setup(replication)?
    Ok(())
}
