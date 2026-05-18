use archive_it::errors::ArchiveItError;
use archive_it::perform::audit::{self, DEFAULT_CONCURRENCY, ExpirationPolicy};
use awsutils::{
    bucket, config,
    file::{self, File},
};
use base::Stack;
use base::stack::DateCtx;
use chrono::{Months, Utc};
use clap::Args as ClapArgs;
use constants::TEXT_CSV;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipres-dev1)
    #[arg(short, long, env = "STACK")]
    stack: String,

    /// S3 key prefix (should match the one used at upload time)
    #[arg(long)]
    key_prefix: Option<String>,

    /// Concurrent HEAD requests
    #[arg(long, default_value_t = DEFAULT_CONCURRENCY)]
    concurrency: usize,

    /// When set, rows whose `store_time` is older than N years are reported
    /// as expired (and tagged, if --expire-tag-key/--expire-tag-value are set).
    #[arg(long)]
    expire_after_years: Option<u32>,

    /// Tag key to apply to expired objects. Requires --expire-after-years and
    /// --expire-tag-value.
    #[arg(long, requires_all = ["expire_after_years", "expire_tag_value"])]
    expire_tag_key: Option<String>,

    /// Tag value to apply to expired objects. Requires --expire-after-years and
    /// --expire-tag-key.
    #[arg(long, requires_all = ["expire_after_years", "expire_tag_key"])]
    expire_tag_value: Option<String>,
}

impl Args {
    fn expiration_policy(&self) -> Option<ExpirationPolicy> {
        let years = self.expire_after_years?;
        let older_than = Utc::now() - Months::new(years * 12);
        let tag = match (self.expire_tag_key.clone(), self.expire_tag_value.clone()) {
            (Some(k), Some(v)) => Some((k, v)),
            _ => None,
        };
        Some(ExpirationPolicy { older_than, tag })
    }
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

    let inventory_file: File = stack.archive_it_inventory(None).into();
    if !file::exists(&s3, &inventory_file).await {
        return Err(ArchiveItError::NotFound(format!(
            "Archive-It inventory CSV not found: {}",
            inventory_file.s3_url()
        ))
        .into());
    }

    let sync_file: File = stack.archive_it_sync(None).into();
    let dated_sync_file: File = stack.archive_it_sync(Some(DateCtx::Today)).into();

    let temp_dir = tempfile::tempdir()?;
    let local_inventory = temp_dir.path().join("warcs.csv");
    let local_sync = temp_dir.path().join("warcs_sync.csv");

    tracing::info!(s3_url = %inventory_file.s3_url(), "Downloading inventory CSV");
    let bytes = file::download_bytes(&s3, &inventory_file).await?;
    tokio::fs::write(&local_inventory, &bytes).await?;

    let expiration = args.expiration_policy();
    let stats = audit::perform(
        &s3,
        &audit::PerformArgs {
            inventory: local_inventory,
            sync_out: local_sync.clone(),
            bucket: archive_it_bucket,
            key_prefix: args.key_prefix,
            concurrency: args.concurrency,
            expiration,
        },
    )
    .await?;

    tracing::info!(
        matched_sha1 = stats.matched_sha1,
        matched_size = stats.matched_size,
        unmatched = stats.unmatched,
        not_found = stats.not_found,
        expired = stats.expired,
        errored = stats.errored,
        skipped = stats.skipped,
        "Archive-It audit complete"
    );

    let body = tokio::fs::read(&local_sync).await?;
    // Dated first: it's the permanent record. If the live upload then fails,
    // the dated snapshot still captures the audit output for this run.
    file::upload(&s3, &dated_sync_file, body.clone(), TEXT_CSV).await?;
    tracing::info!(s3_url = %dated_sync_file.s3_url(), "Uploaded dated sync CSV snapshot");

    file::upload(&s3, &sync_file, body, TEXT_CSV).await?;
    tracing::info!(s3_url = %sync_file.s3_url(), "Uploaded sync CSV");

    Ok(())
}
