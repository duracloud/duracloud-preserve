use aws_sdk_cloudwatch::config::Region;
use awsutils::bucket;
use awsutils::cloudwatch;
use awsutils::config;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Bucket name
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let sdk_config = config::load_defaults().await;
    let s3 = aws_sdk_s3::Client::new(&sdk_config);

    let region = bucket::region(&s3, &args.bucket).await?;

    let cw_config = aws_sdk_cloudwatch::config::Builder::from(&sdk_config)
        .region(Region::new(region.clone()))
        .build();
    let client = aws_sdk_cloudwatch::Client::from_conf(cw_config);

    let metrics = cloudwatch::get_bucket_metrics(&client, &args.bucket).await?;

    println!(
        "Bucket: {} ({})\nUsage: {} files, {} ({} bytes)",
        args.bucket,
        region,
        metrics.total_objects,
        base::format_bytes(metrics.total_bytes),
        metrics.total_bytes
    );

    Ok(())
}
