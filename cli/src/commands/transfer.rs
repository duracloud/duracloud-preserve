use apputils::Stack;
use awsutils::{
    batch::{self},
    bucket::exists,
};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack destination bucket that receives files (e.g., digipress-dev1-private)
    #[arg(short, long)]
    destination: String,

    /// Source bucket that files are transferred from (e.g., source-bucket)
    #[arg(short, long)]
    source: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let destination = args.destination;
    let stack = Stack::from_bucket_name(&destination)?;
    let config = app::config::config(stack.clone()).await?;

    let stack_buckets = app::bucket::get_stack_buckets(config.s3(), &stack).await?;
    if !stack_buckets.iter().any(|b| b.name() == destination) {
        return Err(format!("Destination bucket '{destination}' is not a stack bucket").into());
    }

    let source = args.source;
    if !exists(config.s3(), &source).await {
        return Err("Source bucket not found".into());
    }

    println!("Found buckets:");
    println!("\tSource bucket: {source}");
    println!("\tDestination bucket: {destination}");

    // TODO: Prompt for confirmation

    let job_id = batch::create_copy_job(
        config.s3control(),
        config.account_id(),
        config.batch_role_arn(),
        &source,
        &destination,
        &stack.managed_bucket(),
    )
    .await?;

    println!("\nSuccessfully created transfer job: {job_id}");
    Ok(())
}
