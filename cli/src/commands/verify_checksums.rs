use apputils::Stack;
use awsutils::bucket::exists;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Bucket to verify checksums for (e.g., digipress-dev1-private)
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let bucket = args.bucket;
    let stack = Stack::from_bucket_name(&bucket)?;
    let config = awsutils::config::request_config(stack.clone()).await;

    if !exists(&config.client, &bucket).await {
        return Err("Bucket not found".into());
    }

    dbg!(stack);
    Ok(())
}
