pub mod bucket;
#[cfg(feature = "duckdb")]
pub mod checksum;
pub mod content_type;
pub mod errors;
#[cfg(feature = "duckdb")]
pub mod inventory;
pub mod stack;
pub mod stats;
pub use stack::{ManagedFile, Stack};
pub mod storage;

use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

pub fn current_timestamp() -> Result<u64, SystemTimeError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
