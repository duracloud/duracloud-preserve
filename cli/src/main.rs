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
    /// Check bucket configuration and report drift
    BucketReconciliation(commands::bucket_reconciliation::Args),
    /// Process bucket creation requests
    BucketRequest(commands::bucket_request::Args),
    /// Checksum a file
    Checksum(commands::checksum::Args),
    /// Build checksum inventory from S3 inventory data
    ChecksumInventory(commands::checksum_inventory::Args),
    /// Generate checksum report and statistics
    ChecksumReport(commands::checksum_report::Args),
    /// Run S3 batch operations compute checksums
    ComputeChecksums(commands::compute_checksums::Args),
    /// Generate inventory report and statistics
    InventoryReport(commands::inventory_report::Args),
    /// Reset stack (empty buckets, optionally destroy resources)
    Reset(commands::reset::Args),
    /// Generate storage report
    StorageReport(commands::storage_report::Args),
    /// Transfer files from source to stack destination bucket
    Transfer(commands::transfer::Args),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::BucketReconciliation(args) => commands::bucket_reconciliation::run(args).await?,
        Commands::BucketRequest(args) => commands::bucket_request::run(args).await?,
        Commands::Checksum(args) => commands::checksum::run(args).await?,
        Commands::ChecksumInventory(args) => commands::checksum_inventory::run(args).await?,
        Commands::ChecksumReport(args) => commands::checksum_report::run(args).await?,
        Commands::ComputeChecksums(args) => commands::compute_checksums::run(args).await?,
        Commands::InventoryReport(args) => commands::inventory_report::run(args).await?,
        Commands::Reset(args) => commands::reset::run(args).await?,
        Commands::StorageReport(args) => commands::storage_report::run(args).await?,
        Commands::Transfer(args) => commands::transfer::run(args).await?,
    }

    Ok(())
}
