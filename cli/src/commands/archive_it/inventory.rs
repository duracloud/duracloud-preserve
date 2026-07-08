use archive_it::errors::ArchiveItError;
use archive_it::perform::inventory;
use awsutils::{
    bucket, config,
    file::{self, File},
};
use base::Stack;
use base::stack::DateCtx;
use clap::Args as ClapArgs;
use constants::TEXT_CSV;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipres-dev1)
    #[arg(short, long, env = "STACK")]
    stack: String,

    /// Archive-It account username
    #[arg(long, env = "ARCHIVE_IT_USERNAME")]
    username: String,

    /// Archive-It account password
    #[arg(long, env = "ARCHIVE_IT_PASSWORD", hide_env_values = true)]
    password: String,

    /// Header name sent on every Archive-It request (allow-list token)
    #[arg(long, env = "ARCHIVE_IT_HEADER_NAME", requires = "header_value")]
    header_name: Option<String>,

    /// Header value (the token itself)
    #[arg(
        long,
        env = "ARCHIVE_IT_HEADER_VALUE",
        requires = "header_name",
        hide_env_values = true
    )]
    header_value: Option<String>,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let sdk_config = config::load_defaults().await;
    let s3 = aws_sdk_s3::Client::new(&sdk_config);

    let archive_it_bucket = stack.archive_it_bucket();
    if !bucket::exists(&s3, &archive_it_bucket).await? {
        return Err(ArchiveItError::NotFound(format!(
            "Archive-It bucket not found (does this stack have Archive-It enabled?): {archive_it_bucket}"
        ))
        .into());
    }

    let inventory_file: File = stack.archive_it_inventory(None).into();
    let dated_file: File = stack.archive_it_inventory(Some(DateCtx::Today)).into();
    let temp_dir = tempfile::tempdir()?;
    let local_csv = temp_dir.path().join("warcs.csv");

    if file::exists(&s3, &inventory_file).await? {
        tracing::info!(s3_url = %inventory_file.s3_url(), "Downloading existing inventory for resume");
        let bytes = file::download_bytes(&s3, &inventory_file).await?;
        tokio::fs::write(&local_csv, &bytes).await?;
    }

    let stats = inventory::perform(&inventory::PerformArgs {
        username: args.username,
        password: args.password,
        header: args.header_name.zip(args.header_value),
        output: local_csv.clone(),
    })
    .await?;

    tracing::info!(
        new_warcs = stats.warc_count,
        collections = stats.collection_count,
        skipped_collections = stats.collection_skipped,
        cached_warcs = stats.cache_hit_count,
        path = %local_csv.display(),
        "Inventory build complete"
    );

    if stats.warc_count == 0 {
        tracing::info!("Inventory up-to-date (no updates available)");
        return Ok(());
    }

    let body = tokio::fs::read(&local_csv).await?;
    // Dated first: it's the only permanent historical record. If the live
    // upload then fails, the next run resumes from the older live and catches up.
    file::upload(&s3, &dated_file, body.clone(), TEXT_CSV).await?;
    tracing::info!(s3_url = %dated_file.s3_url(), "Uploaded dated inventory snapshot");

    file::upload(&s3, &inventory_file, body, TEXT_CSV).await?;
    tracing::info!(s3_url = %inventory_file.s3_url(), "Uploaded inventory");

    Ok(())
}
