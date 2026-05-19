use std::path::Path;
use std::process::Command;

use crate::errors::ArchiveItError;

/// Approximate data-row count via `wc -l` (file lines minus header). Used only
/// for `[N/M]` log prefixes; an embedded newline in a quoted field would
/// undercount but never affects the rows the CSV reader actually processes
/// and is very unlikely for this dataset.
pub fn count_data_rows(path: &Path) -> Result<usize, ArchiveItError> {
    let output = Command::new("wc").arg("-l").arg(path).output()?;
    if !output.status.success() {
        return Err(ArchiveItError::Io(std::io::Error::other(format!(
            "wc -l failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))));
    }
    let count = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    Ok(count.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::InventoryRow;
    use csv::WriterBuilder;

    fn fixture_row(filename: &str) -> InventoryRow {
        InventoryRow {
            collection_id: 1,
            collection_name: "Test".into(),
            account_id: 1,
            filename: filename.into(),
            filetype: "warc".into(),
            size_bytes: 0,
            crawl_id: None,
            crawl_time: None,
            crawl_start: None,
            store_time: "2025-01-01T00:00:00Z".into(),
            sha1: None,
            md5: None,
            primary_location: String::new(),
            all_locations: String::new(),
        }
    }

    #[test]
    fn count_data_rows_empty_file_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.csv");
        std::fs::write(&path, "").unwrap();
        assert_eq!(count_data_rows(&path).unwrap(), 0);
    }

    #[test]
    fn count_data_rows_header_only_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("header.csv");
        std::fs::write(&path, "col_a,col_b\n").unwrap();
        assert_eq!(count_data_rows(&path).unwrap(), 0);
    }

    #[test]
    fn count_data_rows_matches_csv_writer_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.csv");
        let mut wtr = WriterBuilder::new()
            .has_headers(true)
            .from_path(&path)
            .unwrap();
        for filename in ["a.warc.gz", "b.warc.gz", "c.warc.gz"] {
            wtr.serialize(fixture_row(filename)).unwrap();
        }
        wtr.flush().unwrap();
        assert_eq!(count_data_rows(&path).unwrap(), 3);
    }
}
