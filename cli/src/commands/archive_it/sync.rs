use archive_it::errors::ArchiveItError;
use archive_it::perform::sync;
use awsutils::{
    bucket, config,
    file::{self, File},
};
use base::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipres-dev1)
    #[arg(short, long, env = "STACK")]
    stack: String,

    /// Archive-It account username
    #[arg(long, env = "ARCHIVE_IT_USERNAME")]
    username: String,

    /// Archive-It account password
    #[arg(long, env = "ARCHIVE_IT_PASSWORD")]
    password: String,

    /// S3 key prefix (should match audit's prefix)
    #[arg(long)]
    key_prefix: Option<String>,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let sdk_config = config::load_defaults().await;
    let s3 = aws_sdk_s3::Client::new(&sdk_config);

    let archive_it_bucket = stack.archive_it_bucket();
    if !bucket::exists(&s3, &archive_it_bucket).await {
        return Err(ArchiveItError::NotFound(format!(
            "Archive-It bucket not found (does this stack have Archive-It enabled?): {archive_it_bucket}"
        ))
        .into());
    }

    let sync_file: File = stack.archive_it_sync().into();
    if !file::exists(&s3, &sync_file).await {
        tracing::info!(
            s3_url = %sync_file.s3_url(),
            "No sync CSV found; nothing to re-sync."
        );
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let local_sync = temp_dir.path().join("warcs_sync.csv");

    tracing::info!(s3_url = %sync_file.s3_url(), "Downloading sync CSV");
    let bytes = file::download_bytes(&s3, &sync_file).await?;
    tokio::fs::write(&local_sync, &bytes).await?;

    // Claim the work by deleting the live sync CSV up front. A concurrent
    // sync that starts now will see no CSV and exit as a no-op; a fresh
    // audit afterward will rebuild a smaller list for anything we don't
    // finish. The client's HEAD-then-skip handles per-object overlap.
    file::delete(&s3, &sync_file).await?;
    tracing::info!(s3_url = %sync_file.s3_url(), "Claimed sync CSV (deleted from S3)");

    sync::perform(
        &s3,
        &sync::PerformArgs {
            username: args.username,
            password: args.password,
            sync_in: local_sync,
            bucket: archive_it_bucket,
            key_prefix: args.key_prefix,
        },
    )
    .await?;

    Ok(())
}
