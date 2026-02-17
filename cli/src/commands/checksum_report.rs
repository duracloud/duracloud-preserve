use app::perform::checksum_report;
use apputils::{Stack, stack::DateCtx};
use awsutils::{bucket::exists, file::File};
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
    let config = app::config::config(stack.clone()).await?;

    if !exists(config.s3(), &bucket).await {
        return Err("Bucket not found".into());
    }

    let file = File::new(
        stack.managed_bucket(),
        stack.metadata_checksums_path(&bucket, DateCtx::Latest),
    );

    let opts = checksum_report::PerformOptions::default();
    let stats = checksum_report::perform(&config, &file, &opts).await?;
    println!("Checksum report complete:");
    println!("\tTotal objects:      {}", stats.total_objects);
    println!("\tMatches:            {}", stats.matches);
    println!("\tMismatches:         {}", stats.mismatches);
    println!("\tMissing replica:    {}", stats.missing_replica);
    println!("\tMissing source:     {}", stats.missing_source);
    println!("\tFailed source:      {}", stats.failed_source);
    println!("\tFailed replication: {}", stats.failed_replication);

    Ok(())
}
