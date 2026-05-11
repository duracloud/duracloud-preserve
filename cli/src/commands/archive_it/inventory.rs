use archive_it::perform::inventory;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(_args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let perform_args = inventory::PerformArgs {};
    inventory::perform(&perform_args).await?;
    Ok(())
}
