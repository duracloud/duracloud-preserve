use apputils::{Stack, stack::DateCtx};
use awsutils::{
    bucket::exists,
    config::RequestConfig,
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
    let config = awsutils::config::request_config(stack.clone()).await;
    let date_ctx = resolve_date_ctx(&config, &bucket).await;

    if !exists(&config.client, &bucket).await {
        return Err("Bucket not found".into());
    }

    let object = format!(
        "manifests/{}/inventory/{}T01-00Z/manifest.json",
        bucket, date_ctx
    );

    let manifest = File::new(config.stack().managed_bucket(), object);
    let stats = inventory_report::perform(&config, &manifest, date_ctx).await?;

    println!(
        "Processed {} files, {} bytes total",
        stats.total_files, stats.total_size
    );

    Ok(())
}

/// Determine which inventory to use: today's if available, otherwise yesterday's.
async fn resolve_date_ctx(config: &RequestConfig, target_bucket: &str) -> DateCtx {
    let today_manifest = File::new(
        config.stack().managed_bucket(),
        format!(
            "manifests/{}/inventory/{}T01-00Z/manifest.json",
            target_bucket,
            DateCtx::Today
        ),
    );

    if file::exists(&config.client, &today_manifest).await {
        DateCtx::Today
    } else {
        DateCtx::Yesterday
    }
}
