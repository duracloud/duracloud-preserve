use app::{bucket, config, perform::bucket_request};
use awsutils::file::{self, File};
use base::{Stack, current_timestamp};
use clap::Args as ClapArgs;
use constants::TEXT_PLAIN;
use std::path::PathBuf;

#[derive(ClapArgs)]
#[command(group(
    clap::ArgGroup::new("bucket_source")
        .required(true)
        .args(["file", "name"])
))]
pub struct Args {
    /// Stack name (e.g., digipres-dev1)
    #[arg(short, long)]
    stack: String,

    /// Path to file containing bucket names (one per line)
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Bucket name, excluding stack name (e.g., special-collections)
    #[arg(short, long)]
    name: Option<String>,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;

    let names = match (args.file, args.name) {
        (Some(f), None) => bucket::get_request_names(f).await?,
        (None, Some(n)) => vec![n],
        _ => vec![],
    };

    if names.is_empty() {
        return Err("No bucket names found".into());
    }

    let config = config::load(stack.clone()).await?;
    let file = File::from(stack.bucket_request_path(&format!("{}.txt", current_timestamp()?)));

    file::upload(
        config.s3(),
        &file,
        names.join("\n").into_bytes(),
        TEXT_PLAIN,
    )
    .await?;

    println!(
        "Uploaded request file to s3://{}/{}",
        file.bucket(),
        file.key()
    );

    let args = bucket_request::PerformArgs {
        trigger_sync_users: true,
        ..bucket_request::PerformArgs::new(file)
    };
    bucket_request::perform(&config, &args).await?;

    println!("All buckets created successfully");
    Ok(())
}
