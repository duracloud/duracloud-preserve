use apputils::StackName;
use awsutils::generate_checksums;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = StackName::new(&args.stack)?;
    let batch_config = awsutils::config::batch_config(stack.clone()).await;
    let request_config = awsutils::config::request_config(stack.clone()).await;

    let receipts = generate_checksums::perform(&batch_config, &request_config).await?;

    println!("Checksum report jobs scheduled:\n");
    for (i, receipt) in receipts.iter().enumerate() {
        println!("\t[{}] {}", i + 1, receipt);
    }

    Ok(())
}
