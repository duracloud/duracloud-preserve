use std::io::{self, Write};

use apputils::Stack;
use awsutils::bucket;
use awsutils::config::default_config;
use clap::Args as ClapArgs;
use rand::Rng;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let sdk_config = default_config().await;
    let s3_client = aws_sdk_s3::Client::new(&sdk_config);

    println!("Discovering buckets for stack: {}", stack.as_str());

    let buckets = bucket::get_stack_buckets(&s3_client, &stack).await?;

    if buckets.is_empty() {
        println!("No buckets found for stack {}", stack.as_str());
        return Ok(());
    }

    println!("\nFound {} bucket(s):", buckets.len());
    for b in &buckets {
        println!("\t{} ({})", b.name(), b.bucket_type());
    }

    println!("\nPlanned actions:");
    println!("\t- Empty all stack buckets");

    let code = generate_confirmation_code();
    println!("\nTo proceed, enter this code: {}", code);
    print!("Confirmation: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim() != code {
        println!("Code does not match. Aborting.");
        return Ok(());
    }

    for b in &buckets {
        let name = b.name();
        print!("\nEmptying {}... ", name);
        io::stdout().flush()?;
        bucket::empty(&s3_client, name).await?;
        println!("done");
    }

    println!("\nAll stack buckets emptied");

    Ok(())
}

/// Generate confirmation code for user input
fn generate_confirmation_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();

    (0..6)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
