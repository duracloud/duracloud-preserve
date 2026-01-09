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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::BucketRequest(args) => commands::bucket_request::run(args).await?,
    }

    Ok(())
}
