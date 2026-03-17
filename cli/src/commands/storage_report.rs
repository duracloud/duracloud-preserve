use app::{config, perform::storage_report};
use apputils::Stack;
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

    let storage_capacity = if config.storage_capacity() > 0 {
        Some(config.storage_capacity())
    } else {
        None
    };

    let opts = storage_report::PerformOptions {
        storage_capacity_bytes: storage_capacity,
    };
    let stats = storage_report::perform(&config, &opts).await?;

    println!(
        "Usage: {} files, {} bytes total",
        stats.total_files, stats.total_size
    );

    Ok(())
}
