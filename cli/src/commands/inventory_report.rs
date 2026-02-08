use apputils::{Stack, stack::DateCtx};
use awsutils::{
    bucket::exists,
    config::Config,
    file::{self, File},
    inventory_report,
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
    let config = awsutils::config::config(stack.clone()).await;
    let date_ctx = resolve_date_ctx(&config, &bucket).await?;

    if !exists(config.s3(), &bucket).await {
        return Err("Bucket not found".into());
    }

    let object = format!(
        "manifests/{}/inventory/{}T01-00Z/manifest.json",
        bucket, date_ctx
    );

    let manifest = File::new(config.stack().managed_bucket(), object);
    let opts = inventory_report::PerformOptions { date_ctx };
    let stats = inventory_report::perform(&config, &manifest, &opts).await?;

    println!(
        "Processed {} files, {} bytes total",
        stats.total_files, stats.total_size
    );

    Ok(())
}

/// Determine which inventory to use: today's if available, otherwise yesterday's.
async fn resolve_date_ctx(config: &Config, target_bucket: &str) -> Result<DateCtx, &'static str> {
    for ctx in [DateCtx::Today, DateCtx::Yesterday] {
        let manifest = File::new(
            config.stack().managed_bucket(),
            format!(
                "manifests/{}/inventory/{}T01-00Z/manifest.json",
                target_bucket, ctx
            ),
        );
        if file::exists(config.s3(), &manifest).await {
            return Ok(ctx);
        }
    }
    Err("No inventory manifest found for today or yesterday")
}
