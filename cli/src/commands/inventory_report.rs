use app::{
    config::{self, Config},
    perform::inventory_report,
};
use apputils::{Stack, stack::DateCtx};
use awsutils::{
    bucket,
    file::{self, File},
};
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

    if !bucket::exists(config.s3(), &bucket).await {
        return Err("Bucket not found".into());
    }

    let manifest = get_manifest(&config, &bucket).await?;
    let opts = inventory_report::PerformOptions::default();
    let stats = inventory_report::perform(&config, &manifest, &opts).await?;

    println!(
        "Processed {} files, {} bytes total",
        stats.total_files, stats.total_size
    );

    Ok(())
}

/// Determine which inventory to use: today's if available, otherwise yesterday's.
async fn get_manifest(config: &Config, target_bucket: &str) -> Result<File, &'static str> {
    for ctx in [DateCtx::Today, DateCtx::Yesterday] {
        let manifest = File::from(config.stack().inventory_manifest_path(target_bucket, ctx));
        println!("Checking for manifest: {}", manifest.s3_url());
        if file::exists(config.s3(), &manifest).await {
            return Ok(manifest);
        }
    }
    Err("No inventory manifest found for today or yesterday")
}
