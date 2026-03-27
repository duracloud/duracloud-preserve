pub mod bucket;
#[cfg(feature = "duckdb")]
pub mod checksum;
pub mod content_type;
pub mod errors;
#[cfg(feature = "duckdb")]
pub mod inventory;
pub mod stack;
pub mod stats;
use rand::RngExt;
pub use stack::{ManagedFile, Stack};
pub mod storage;

use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

pub fn current_timestamp() -> Result<u64, SystemTimeError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

/// Generate confirmation code for user input
pub fn generate_confirmation_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();

    (0..6)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
