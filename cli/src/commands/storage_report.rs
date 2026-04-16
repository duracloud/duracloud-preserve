use app::{config, perform::storage_report};
use base::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let config = config::load(stack.clone()).await?;

    let args = storage_report::PerformArgs {
        storage_capacity_bytes: Some(config.storage_capacity()),
    };
    let stats = storage_report::perform(&config, &args).await?;

    println!(
        "Usage: {} files, {} bytes total",
        stats.data.total_files, stats.data.total_size
    );

    Ok(())
}
