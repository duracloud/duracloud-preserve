use std::collections::{HashMap, HashSet};
use std::path::Path;

use csv::ReaderBuilder;

use super::InventoryRow;

/// Resume state derived from an existing inventory CSV: per-collection
/// counts and a (collection_id, filename) set so we can skip rows we've
/// already written.
#[derive(Debug, Default)]
pub struct CacheState {
    pub counts: HashMap<u64, u64>,
    pub seen: HashSet<(u64, String)>,
}

pub fn read_cache_state(path: &Path) -> Result<CacheState, csv::Error> {
    let mut rdr = ReaderBuilder::new().has_headers(true).from_path(path)?;
    let mut state = CacheState::default();
    for row in rdr.deserialize::<InventoryRow>() {
        let row = row?;
        if state.seen.insert((row.collection_id, row.filename)) {
            *state.counts.entry(row.collection_id).or_insert(0) += 1;
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use csv::WriterBuilder;

    fn fixture_row(collection_id: u64, filename: &str) -> InventoryRow {
        InventoryRow {
            collection_id,
            collection_name: format!("Collection {collection_id}"),
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
    fn read_cache_state_counts_per_collection_and_dedups() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("warcs.csv");

        let rows = [
            fixture_row(1, "a.warc.gz"),
            fixture_row(1, "b.warc.gz"),
            fixture_row(2, "c.warc.gz"),
            // Duplicate of the first row — must NOT inflate counts or seen.
            fixture_row(1, "a.warc.gz"),
        ];
        let mut wtr = WriterBuilder::new()
            .has_headers(true)
            .from_path(&path)
            .unwrap();
        for row in &rows {
            wtr.serialize(row).unwrap();
        }
        wtr.flush().unwrap();

        let state = read_cache_state(&path).unwrap();
        assert_eq!(state.counts.get(&1).copied(), Some(2));
        assert_eq!(state.counts.get(&2).copied(), Some(1));
        assert_eq!(state.seen.len(), 3);
        assert!(state.seen.contains(&(1, "a.warc.gz".into())));
        assert!(state.seen.contains(&(1, "b.warc.gz".into())));
        assert!(state.seen.contains(&(2, "c.warc.gz".into())));
    }
}
