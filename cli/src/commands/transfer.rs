use app::{bucket as app_bucket, config};
use awsutils::{
    batch::{self},
    bucket,
};
use base::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack destination bucket that receives files (e.g., digipres-dev1-private)
    #[arg(short, long)]
    destination: String,

    /// Optional key prefix at the destination (e.g., put_data_in_here)
    #[arg(short, long)]
    prefix: Option<String>,

    /// Source bucket that files are transferred from (e.g., source-bucket)
    #[arg(short, long)]
    source: String,

    /// Override (i.e. do not) prompt for confirmation
    #[arg(short, long, default_value_t = false)]
    force: bool,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let destination = args.destination;
    let stack = Stack::from_prefixed_name(&destination)?;
    let config = config::load(stack.clone()).await?;

    let stack_buckets = app_bucket::list_for_stack(config.s3(), &stack, None).await?;
    if !stack_buckets.iter().any(|b| b.name() == destination) {
        return Err(format!("Destination bucket '{destination}' is not a stack bucket").into());
    }

    let source = args.source;
    if !bucket::exists(config.s3(), &source).await? {
        return Err("Source bucket not found".into());
    }

    let target_prefix = args.prefix.and_then(|p| {
        let trimmed = p.trim_matches('/').to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    });

    println!("Found buckets:");
    println!("\tSource bucket: {source}");
    println!("\tDestination bucket: {destination}");
    if let Some(prefix) = &target_prefix {
        println!("\tDestination prefix: {prefix}");
    }

    let confirmed = args.force || base::confirm_action()?;
    if !confirmed {
        println!("Code does not match. Aborting.");
        return Ok(());
    }

    let batch_config = batch::BatchConfig {
        client: config.s3control(),
        account_id: config.account_id(),
        role_arn: config.batch_role_arn(),
        stack: &stack,
    };

    let job_id = batch::create_copy_job(
        &batch_config,
        &source,
        &destination,
        target_prefix.as_deref(),
    )
    .await?;

    println!("\nSuccessfully created transfer job: {job_id}");
    Ok(())
}
