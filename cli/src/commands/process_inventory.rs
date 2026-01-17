use apputils::{StackName, stack::DateCtx};
use awsutils::{file::File, process_inventory};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,

    /// Bucket to process inventory for (e.g., digipress-dev1-private)
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = StackName::new(&args.stack)?;
    let bucket = stack.managed_bucket();

    let object = format!(
        "manifests/{}/inventory/{}T01-00Z/manifest.json",
        args.bucket,
        DateCtx::Yesterday
    );

    let manifest = File::new(bucket, object);

    let config = awsutils::config::request_config(stack.clone()).await;
    let stats = process_inventory::perform(&config, &manifest).await?;

    println!(
        "Processed {} files, {} bytes total",
        stats.total_files, stats.total_size
    );
    Ok(())
}
