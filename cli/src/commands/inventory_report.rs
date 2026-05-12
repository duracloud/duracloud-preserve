use app::{
    config::{self},
    inventory,
    perform::inventory_report,
};
use awsutils::bucket;
use base::Stack;
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

    if !bucket::exists(config.s3(), &bucket).await {
        return Err("Bucket not found".into());
    }

    let manifest = inventory::get_manifest(&config, &bucket).await?;
    let args = inventory_report::PerformArgs::new(manifest);
    let stats = inventory_report::perform(&config, &args).await?;

    println!(
        "Processed {} files, {} bytes total",
        stats.total_files, stats.total_size
    );

    Ok(())
}
