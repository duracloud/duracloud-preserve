use apputils::{Stack, stack::DateCtx};
use awsutils::{config::RequestConfig, file, file::File, process_inventory};
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
    let stack = Stack::new(&args.stack)?;
    let config = awsutils::config::request_config(stack.clone()).await;
    let date_ctx = resolve_date_ctx(&config, &args.bucket).await;

    let object = format!(
        "manifests/{}/inventory/{}T01-00Z/manifest.json",
        args.bucket, date_ctx
    );

    let manifest = File::new(config.stack().managed_bucket(), object);
    let stats = process_inventory::perform(&config, &manifest, date_ctx).await?;

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
