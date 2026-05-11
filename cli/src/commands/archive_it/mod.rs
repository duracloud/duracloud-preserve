pub mod audit;
pub mod inventory;
pub mod sync;

use clap::{Args as ClapArgs, Subcommand};

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Audit Archive-It holdings against the inventory
    Audit(audit::Args),
    /// Build Archive-It inventory
    Inventory(inventory::Args),
    /// Sync Archive-It content
    Sync(sync::Args),
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        Commands::Audit(a) => audit::run(a).await,
        Commands::Inventory(a) => inventory::run(a).await,
        Commands::Sync(a) => sync::run(a).await,
    }
}
