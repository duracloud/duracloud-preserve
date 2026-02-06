use apputils::Stack;
use awsutils::{bucket, compute_checksums};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Bucket to compute checksums for (e.g., digipress-dev1-private)
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let bucket = args.bucket;
    let stack = Stack::from_bucket_name(&bucket)?;
    let config = awsutils::config::config(stack).await;
    let bucket_name = bucket::Name::new(&bucket)?;

    let receipts = compute_checksums::perform(&config, Some(&bucket_name)).await?;

    println!("Compute checksums jobs scheduled:\n");
    for (i, receipt) in receipts.iter().enumerate() {
        println!("\t[{}] {}", i + 1, receipt);
    }

    Ok(())
}
