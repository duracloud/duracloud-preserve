use app::{config, perform::checksum_inventory};
use awsutils::file::{self, File};
use base::{Stack, stack::DateCtx};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Bucket to process inventory for (e.g., digipress-dev1-private)
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let bucket = args.bucket;
    let stack = Stack::from_bucket_name(&bucket)?;
    let config = config::load(stack.clone()).await?;

    let report = File::from(
        config
            .stack()
            .reports_manifests_path(&bucket, DateCtx::Latest),
    );

    if !file::exists(config.s3(), &report).await {
        return Err("Inventory report not found".into());
    }

    let args = checksum_inventory::PerformArgs::new(report);
    let inventory = checksum_inventory::perform(&config, &args).await?;

    println!("Checksum inventory uploaded to: {inventory}");

    Ok(())
}
