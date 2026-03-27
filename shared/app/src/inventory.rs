use apputils::stack::DateCtx;
use awsutils::file::{self, File};

use crate::config::Config;

/// Determine which inventory to use: today's if available, otherwise yesterday's.
pub async fn get_manifest(config: &Config, target_bucket: &str) -> Result<File, &'static str> {
    for ctx in [DateCtx::Today, DateCtx::Yesterday] {
        let manifest = File::from(config.stack().inventory_manifest_path(target_bucket, ctx));
        println!("Checking for manifest: {}", manifest.s3_url());
        if file::exists(config.s3(), &manifest).await {
            return Ok(manifest);
        }
    }
    Err("No inventory manifest found for today or yesterday")
}
