use archive_it::perform::sync;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(_args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let perform_args = sync::PerformArgs {};
    sync::perform(&perform_args).await?;
    Ok(())
}
