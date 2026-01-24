mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "duracloud")]
#[command(about = "CLI for duracloud operations", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Process bucket creation requests
    BucketRequest(commands::bucket_request::Args),
    /// Generate checksum reports
    GenerateChecksums(commands::generate_checksums::Args),
    /// Process inventory
    ProcessInventory(commands::process_inventory::Args),
    /// Reset stack (empty buckets, optionally destroy resources)
    Reset(commands::reset::Args),
    /// Set up a new stack (IAM roles, managed bucket, request bucket)
    Setup(commands::setup::Args),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::BucketRequest(args) => commands::bucket_request::run(args).await?,
        Commands::GenerateChecksums(args) => commands::generate_checksums::run(args).await?,
        Commands::ProcessInventory(args) => commands::process_inventory::run(args).await?,
        Commands::Reset(args) => commands::reset::run(args).await?,
        Commands::Setup(args) => commands::setup::run(args).await?,
    }

    Ok(())
}
