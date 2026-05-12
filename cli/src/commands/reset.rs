use std::io::{self, Write};

use app::bucket as app_bucket;
use awsutils::bucket::{self as aws_bucket, Type};
use awsutils::config;
use base::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipres-dev1)
    #[arg(short, long)]
    stack: String,

    /// Bucket to delete (e.g., digipres-dev1-private)
    #[arg(short, long)]
    bucket: Option<String>,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let sdk_config = config::load_defaults().await;
    let s3_client = aws_sdk_s3::Client::new(&sdk_config);

    let buckets = match &args.bucket {
        Some(bucket) => aws_bucket::from_name(&s3_client, bucket)
            .await?
            .into_iter()
            .collect(),
        None => {
            println!("Discovering buckets for stack: {}", stack.as_str());
            app_bucket::list_for_stack(&s3_client, &stack, None).await?
        }
    };

    if buckets.is_empty() {
        println!("No buckets found for stack {}", stack.as_str());
        return Ok(());
    }

    let (internal, non_internal): (Vec<_>, Vec<_>) = buckets
        .iter()
        .partition(|b| *b.bucket_type() == Type::Internal);

    println!("\nFound {} bucket(s):", buckets.len());
    for b in &buckets {
        println!("\t{} ({})", b.name(), b.bucket_type());
    }

    println!("\nPlanned actions:");
    println!("\t- Empty {} internal bucket(s)", internal.len());
    println!("\t- Delete {} non-internal bucket(s)", non_internal.len());

    if !base::confirm_action()? {
        println!("Code does not match. Aborting.");
        return Ok(());
    }

    // Empty internal buckets
    for b in &internal {
        let name = b.name();
        print!("\nEmptying {}... ", name);
        io::stdout().flush()?;
        aws_bucket::empty(&s3_client, name).await?;
        println!("done");
    }

    // Delete non-internal buckets (empty then delete)
    for b in &non_internal {
        let name = b.name();
        print!("\nDeleting {}... ", name);
        io::stdout().flush()?;
        aws_bucket::empty(&s3_client, name).await?;
        aws_bucket::delete(&s3_client, name).await?;
        println!("done");
    }

    println!("\nReset complete");

    Ok(())
}
