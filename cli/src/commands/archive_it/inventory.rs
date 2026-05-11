use archive_it::perform::inventory;
use awsutils::config;
use base::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,

    /// Archive-It account username
    #[arg(long, env = "ARCHIVE_IT_USERNAME")]
    username: String,

    /// Archive-It account password
    #[arg(long, env = "ARCHIVE_IT_PASSWORD")]
    password: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let sdk_config = config::load_defaults().await;
    let s3 = aws_sdk_s3::Client::new(&sdk_config);

    let perform_args = inventory::PerformArgs {
        username: args.username,
        password: args.password,
    };
    inventory::perform(&s3, &stack, &perform_args).await?;
    Ok(())
}
