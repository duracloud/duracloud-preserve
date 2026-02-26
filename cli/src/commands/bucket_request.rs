use std::path::PathBuf;

use app::perform::bucket_request;
use apputils::{Stack, content_type};
use awsutils::file::File;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
#[command(group(
    clap::ArgGroup::new("bucket_source")
        .required(true)
        .args(["file", "name"])
))]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
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

    let (content, filename) = resolve_names(args.file, args.name).await?;
    if content.lines().filter(|s| !s.is_empty()).count() == 0 {
        return Err("No bucket names found in file".into());
    }

    let config = app::config::config(stack.clone()).await?;

    // Note: we upload to the managed bucket (not the request bucket) intentionally
    // because if the function is deployed we don't want to process twice
    let file = File::new(
        stack.managed_bucket(),
        format!("bucket-request/{}", filename),
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

async fn resolve_names(
    file: Option<PathBuf>,
    name: Option<String>,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    match (file, name) {
        (Some(path), None) => {
            let expanded = shellexpand::tilde(&path.to_string_lossy()).into_owned();
            let content = tokio::fs::read_to_string(&expanded).await?;
            let filename = path
                .file_name()
                .ok_or("invalid filename")?
                .to_string_lossy()
                .into_owned();
            Ok((content, filename))
        }
        (None, Some(name)) => Ok((format!("{name}\n"), format!("{name}.txt"))),
        _ => unreachable!(),
    }
}
