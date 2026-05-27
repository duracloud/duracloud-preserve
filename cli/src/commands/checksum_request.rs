use app::{config, perform::checksum_request};
use awsutils::file::{self, File};
use base::{Stack, stack::DateCtx};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Bucket to process inventory for (e.g., digipres-dev1-private)
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let bucket = args.bucket;
    let stack = Stack::from_prefixed_name(&bucket)?;
    let config = config::load(stack.clone()).await?;

    let report = File::from(
        config
            .stack()
            .reports_manifests_path(&bucket, DateCtx::Latest),
    );

    if !file::exists(config.s3(), &report).await? {
        return Err(format!("Inventory report not found for bucket: {bucket}").into());
    }

    let args = checksum_request::PerformArgs::new(report);
    let inventory = match checksum_request::perform(&config, &args).await {
        Ok(inventory) => inventory,
        Err(e) => {
            eprintln!("Error processing checksum request: {e}");
            return Err(e.into());
        }
    };

    println!("Checksum inventory uploaded to: {inventory}");

    Ok(())
}
