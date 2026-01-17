use apputils::StackName;
use awsutils::{file::File, inventory};
use chrono::Utc;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,

    /// Bucket to process inventory for (e.g., digipress-dev1-private)
    #[arg(short, long)]
    bucket: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = StackName::new(&args.stack)?;
    let bucket = stack.managed_bucket();
    let date = (Utc::now() - chrono::Duration::days(1)).format("%Y-%m-%d");
    let object = format!(
        "manifests/{}/inventory/{}T01-00Z/manifest.json",
        args.bucket, date
    );
    let manifest = File::new(bucket, object);

    // dbg!(&manifest);

    let config = awsutils::config::request_config(stack.clone()).await;
    inventory::perform(&config, &manifest).await?;
    println!("Inventory processed successfully");
    Ok(())
}
