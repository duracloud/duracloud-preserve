use app::{config, perform::compute_checksums};
use base::Stack;
use base::bucket;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Bucket to compute checksums for (e.g., digipress-dev1-private)
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let bucket = args.bucket;
    let stack = Stack::from_prefixed_name(&bucket)?;
    let config = config::load(stack).await?;
    let bucket_name = bucket::Name::new(&bucket)?;

    let args = compute_checksums::PerformArgs::for_bucket(bucket_name);
    let receipts = compute_checksums::perform(&config, &args).await?;

    println!("Compute checksums jobs scheduled:\n");
    for (i, receipt) in receipts.iter().enumerate() {
        println!("\t[{}] {}", i + 1, receipt);
    }

    Ok(())
}
