pub mod bucket;
#[cfg(feature = "duckdb")]
pub mod checksum;
pub mod errors;
#[cfg(feature = "duckdb")]
pub mod inventory;
pub mod stack;
pub mod stats;
use base64::{Engine, engine::general_purpose};
use crc_fast::{CrcAlgorithm, Digest};
use humansize::DECIMAL;
use rand::RngExt;
pub use stack::{ManagedFile, Stack};
pub mod storage;

use std::{
    io::{self, BufRead},
    time::{SystemTime, SystemTimeError, UNIX_EPOCH},
};

pub fn confirm_action() -> io::Result<bool> {
    use io::Write;

    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();
    let code: String = (0..6)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    println!("\nTo proceed, enter this code: {code}");
    print!("Confirmation: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim() == code)
}

pub fn current_timestamp() -> Result<u64, SystemTimeError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

/// Format a byte count as a human-readable SI string (e.g. "1.5 GB").
pub fn format_bytes(value: u64) -> String {
    humansize::format_size(value, DECIMAL)
}

pub fn generate_checksum(mut reader: impl BufRead, algorithm: CrcAlgorithm) -> io::Result<String> {
    let mut buffer = [0u8; 8192];
    let mut digest = Digest::new(algorithm);

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }

    let bytes = digest.finalize().to_be_bytes();

    // Base64 encode to match AWS S3 console
    Ok(general_purpose::STANDARD.encode(bytes))
}

/// Format strings as a comma-separated list of SQL string literals for DuckDB,
/// single-quoting each value and escaping embedded quotes.
pub fn safe_join(files: &[impl AsRef<str>]) -> String {
    files
        .iter()
        .map(|f| format!("'{}'", f.as_ref().replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ")
}
