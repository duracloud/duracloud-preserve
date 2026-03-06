use app::perform::storage_report;
use apputils::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,
    /// Optional total storage capacity in bytes
    #[arg(long)]
    storage_capacity_bytes: Option<u64>,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let config = app::config::config(stack.clone()).await?;

    let opts = storage_report::PerformOptions {
        storage_capacity_bytes: args.storage_capacity_bytes,
    };
    let stats = storage_report::perform(&config, &opts).await?;
    println!("{:?}", stats);

    Ok(())
}
