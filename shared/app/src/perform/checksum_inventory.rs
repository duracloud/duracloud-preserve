use awsutils::file::File;

use crate::{config::Config, perform::errors::ChecksumInventoryError};

#[derive(Debug, Clone, Copy, Default)]
pub struct PerformOptions {}

pub async fn perform(
    config: &Config,
    report: &File,
    opts: &PerformOptions,
) -> Result<(), ChecksumInventoryError> {
    let _ = (config, report, opts);
    tracing::info!("checksum_inventory: not yet implemented");
    Ok(())
}
