pub mod list;
pub mod run;

use clap::{Args as ClapArgs, Subcommand};

/// Scheduler group holding stack task schedules; must match
/// `terraform/modules/stack/tasks.tf`.
const SCHEDULE_GROUP: &str = "default";

/// Cluster name for a stack; must match `terraform/modules/stack/tasks.tf`.
fn cluster_name(stack: &str) -> String {
    format!("{stack}-tasks")
}

/// Task definition family / schedule name for a stack task; must match
/// `terraform/modules/stack/tasks.tf`.
fn family_name(stack: &str, task: &str) -> String {
    format!("{stack}-{task}")
}

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List the stack's deployed tasks and their schedules
    List(list::Args),
    /// Launch a deployed task on demand (replays its schedule target)
    Run(run::Args),
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        Commands::List(a) => list::run(a).await,
        Commands::Run(a) => run::run(a).await,
    }
}
