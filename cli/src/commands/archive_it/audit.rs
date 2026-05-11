use archive_it::perform::audit;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(_args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let perform_args = audit::PerformArgs {};
    audit::perform(&perform_args).await?;
    Ok(())
}
