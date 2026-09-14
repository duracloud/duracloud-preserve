use app::{config, perform::compute_checksums};
use base::Stack;
use base::bucket;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack to compute checksums for across all buckets (requires confirmation)
    #[arg(
        short,
        long,
        required_unless_present = "bucket",
        conflicts_with = "bucket"
    )]
    stack: Option<String>,

    /// Bucket to compute checksums for (e.g., digipres-dev1-private)
    #[arg(short, long)]
    bucket: Option<String>,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = match (&args.stack, &args.bucket) {
        (Some(stack), None) => Stack::new(stack)?,
        (None, Some(bucket)) => Stack::from_prefixed_name(bucket)?,
        _ => return Err("Specify either --stack or --bucket".into()),
    };
    let bucket = args.bucket.as_deref().map(bucket::Name::new).transpose()?;

    if bucket.is_none() {
        println!(
            "Compute checksums for all buckets in stack: {}",
            stack.as_str()
        );
        if !base::confirm_action()? {
            println!("Code does not match. Aborting.");
            return Ok(());
        }
    }

    let config = config::load(stack).await?;

    let args = compute_checksums::PerformArgs { bucket };
    let receipts = compute_checksums::perform(&config, &args).await?;

    println!("Compute checksums jobs scheduled:\n");
    for (i, receipt) in receipts.iter().enumerate() {
        println!("\t[{}] {}", i + 1, receipt);
    }

    Ok(())
}
