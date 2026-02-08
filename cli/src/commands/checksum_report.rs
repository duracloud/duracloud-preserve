use apputils::{Stack, stack::DateCtx};
use awsutils::{bucket::exists, checksum_report, file::File};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Bucket to generate checksum report for (e.g., digipress-dev1-private)
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let bucket = args.bucket;
    let stack = Stack::from_bucket_name(&bucket)?;
    let config = awsutils::config::config(stack.clone()).await;

    if !exists(config.s3(), &bucket).await {
        return Err("Bucket not found".into());
    }

    let file = File::new(
        stack.managed_bucket(),
        stack.metadata_checksums_path(&bucket, DateCtx::Latest),
    );

    // TODO: return report location
    let opts = checksum_report::PerformOptions::default();
    checksum_report::perform(&config, &file, &opts).await?;

    Ok(())
}
