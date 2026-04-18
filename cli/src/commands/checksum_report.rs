use app::{config, perform::checksum_report};
use awsutils::{bucket, file::File};
use base::{Stack, stack::DateCtx};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Bucket to generate checksum report for (e.g., digipress-dev1-private)
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

    let file = File::from(stack.metadata_checksums_receipts_path(&bucket, DateCtx::Latest));

    let args = checksum_report::PerformArgs::new(file);
    let stats = checksum_report::perform(&config, &args).await?;
    println!("Checksum report complete:");
    println!("\tTotal objects:      {}", stats.total_objects());
    println!("\tMatches:            {}", stats.matches);
    println!("\tMismatches:         {}", stats.mismatches);
    println!("\tMissing replica:    {}", stats.missing_replica);
    println!("\tMissing source:     {}", stats.missing_source);
    println!("\tFailed source:      {}", stats.failed_source);
    println!("\tFailed replication: {}", stats.failed_replication);

    Ok(())
}
