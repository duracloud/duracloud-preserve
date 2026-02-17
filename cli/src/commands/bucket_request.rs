use std::path::PathBuf;

use app::perform::bucket_request;
use apputils::{Stack, content_type};
use awsutils::file::File;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,

    /// Path to file containing bucket names (one per line)
    #[arg(short, long)]
    names: PathBuf,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;

    let names = shellexpand::tilde(&args.names.to_string_lossy()).into_owned();
    let content = tokio::fs::read_to_string(&names).await?;
    if content.lines().filter(|s| !s.is_empty()).count() == 0 {
        return Err("No bucket names found in file".into());
    }

    let config = app::config::config(stack.clone()).await;

    let filename = args
        .names
        .file_name()
        .ok_or("invalid filename")?
        .to_string_lossy();

    // Note: we upload to the managed bucket (not the request bucket) intentionally
    // because if the function is deployed we don't want to process twice
    let file = File::new(
        stack.managed_bucket(),
        format!("bucket-request/{}", filename.into_owned()),
    );

    awsutils::file::upload(
        config.s3(),
        &file,
        content.into_bytes(),
        content_type::TEXT_PLAIN,
    )
    .await?;

    println!(
        "Uploaded request file to s3://{}/{}",
        file.bucket(),
        file.key()
    );

    let opts = app::perform::bucket_request::PerformOptions::default();
    bucket_request::perform(&config, &file, &opts).await?;

    println!("All buckets created successfully");
    Ok(())
}
