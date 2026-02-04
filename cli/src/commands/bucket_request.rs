use std::path::PathBuf;

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

    let config = awsutils::config::request_config(stack.clone()).await;

    let filename = args
        .names
        .file_name()
        .ok_or("invalid filename")?
        .to_string_lossy();
    let file = File::new(stack.request_bucket(), filename.into_owned());

    awsutils::file::upload(
        &config.client,
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

    awsutils::bucket_request::perform(&config, &file).await?;
    println!("All buckets created successfully");
    Ok(())
}
