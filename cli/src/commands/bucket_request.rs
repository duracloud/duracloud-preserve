use std::path::PathBuf;

use apputils::StackName;
use awsutils::bucket::{self};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(long)]
    stack: String,

    /// Path to file containing bucket names (one per line)
    #[arg(long)]
    names: PathBuf,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = StackName::new(&args.stack)?;

    let names: Vec<String> = std::fs::read_to_string(&args.names)?
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if names.is_empty() {
        return Err("No bucket names found in file".into());
    }

    println!("Stack: {}", stack.as_str());
    println!("Bucket names to process: {:?}", names);

    let request_config = awsutils::config::request_config(stack).await;
    let buckets = bucket::review_bucket_names(&request_config, &names)?;

    println!("Buckets to create:");
    for (primary, replication) in &buckets {
        println!("\tPrimary: {} ({})", primary.0.as_str(), primary.1);
        println!(
            "\tReplication: {} ({})",
            replication.0.as_str(),
            replication.1
        );
    }

    let issues = bucket::create_buckets(&request_config, &buckets).await;
    if !issues.is_empty() {
        eprintln!("Errors creating buckets:");
        for issue in &issues {
            eprintln!("  {}", issue);
        }
        return Err("Failed to create one or more buckets".into());
    }

    println!("All buckets created successfully");
    Ok(())
}
