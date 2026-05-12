use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::pin::pin;

use archive_it_client::wasapi::DEFAULT_WEBDATA_PAGE_SIZE;
use archive_it_client::{PartnerClient, WasapiClient, WebdataQuery};
use csv::{ReaderBuilder, Terminator, WriterBuilder};
use futures::TryStreamExt;

use crate::errors::ArchiveItError;
use crate::inventory::InventoryRow;

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub username: String,
    pub password: String,
    pub output: PathBuf,
}

pub async fn perform(args: &PerformArgs) -> Result<(), ArchiveItError> {
    let partner = PartnerClient::new(args.username.clone(), args.password.clone())?;
    let wasapi = WasapiClient::new(args.username.clone(), args.password.clone())?;

    let output = args.output.as_path();
    let header_needed = std::fs::metadata(output)
        .map(|m| m.len() == 0)
        .unwrap_or(true);
    let mut state = if header_needed {
        CacheState::default()
    } else {
        read_cache_state(output)?
    };
    if !state.seen.is_empty() {
        tracing::info!(
            cached_files = state.seen.len(),
            cached_collections = state.counts.len(),
            path = %output.display(),
            "Resuming from cached inventory"
        );
    }

    let file = OpenOptions::new().create(true).append(true).open(output)?;
    let mut writer = WriterBuilder::new()
        .has_headers(header_needed)
        .terminator(Terminator::Any(b'\n'))
        .from_writer(file);

    let mut collections = pin!(partner.collections());
    let mut collection_count = 0_u64;
    let mut collection_skipped = 0_u64;
    let mut warc_count = 0_u64;
    let mut cache_hit_count = 0_u64;

    while let Some(collection) = collections.try_next().await? {
        let cached = state.counts.get(&collection.id).copied().unwrap_or(0);

        let query = WebdataQuery {
            collection: Some(collection.id),
            filetype: Some("warc".to_owned()),
            page_size: Some(DEFAULT_WEBDATA_PAGE_SIZE),
            ..Default::default()
        };

        let mut page = wasapi.list_webdata(&query).await?;
        let total = page.count;

        // First-page short-circuit: skip only when the cache exactly matches
        // the API count. Net-decrease (cached > total) means deletions on
        // the API side, which can hide additions — paginate fully to catch
        // them.
        if total > 0 && cached == total {
            collection_skipped += 1;
            cache_hit_count += cached;
            tracing::info!(
                collection_id = collection.id,
                collection_name = %collection.name,
                cached,
                "Collection fully cached, skipping"
            );
            continue;
        }

        if cached > total {
            tracing::warn!(
                collection_id = collection.id,
                collection_name = %collection.name,
                cached,
                api_total = total,
                "Cache exceeds API count (possible deletions); paginating fully"
            );
        }

        collection_count += 1;
        let expected_new = total.saturating_sub(cached);
        tracing::info!(
            collection_id = collection.id,
            collection_name = %collection.name,
            api_total = total,
            cached,
            expected_new,
            "Processing collection"
        );

        let mut written_in_collection = 0_u64;
        let mut cached_in_collection = 0_u64;

        loop {
            let mut written_this_page = 0_u64;
            let mut cached_this_page = 0_u64;

            for file in page.files.drain(..) {
                if file.filetype != "warc" {
                    continue;
                }
                if !state.seen.insert((collection.id, file.filename.clone())) {
                    cached_this_page += 1;
                    continue;
                }

                let primary_location = wasapi.primary_location(&file).unwrap_or_default();
                let row = InventoryRow::from_wasapi(&file, &collection.name, primary_location);
                writer.serialize(&row)?;
                written_this_page += 1;
            }

            // Flush after each page so successfully-fetched rows are durable
            // even if the next page errors out.
            writer.flush()?;

            written_in_collection += written_this_page;
            cached_in_collection += cached_this_page;

            // Once we've found at least the number of new files the API's
            // count delta implies, stop paginating.
            if expected_new > 0 && written_in_collection >= expected_new {
                tracing::debug!(
                    collection_id = collection.id,
                    written = written_in_collection,
                    expected_new,
                    "Found expected new files; stopping pagination"
                );
                break;
            }

            match wasapi.list_webdata_next(&page).await? {
                Some(next) => page = next,
                None => break,
            }
        }

        warc_count += written_in_collection;
        cache_hit_count += cached_in_collection;
    }

    writer.flush()?;
    tracing::info!(
        new_warcs = warc_count,
        collections = collection_count,
        skipped_collections = collection_skipped,
        cached_warcs = cache_hit_count,
        path = %output.display(),
        "Inventory build complete"
    );

    Ok(())
}

/// Resume state derived from an existing inventory CSV.
#[derive(Debug, Default)]
struct CacheState {
    /// Cached file count per collection_id.
    counts: HashMap<u64, u64>,
    /// (collection_id, filename) of rows already in the CSV.
    seen: HashSet<(u64, String)>,
}

fn read_cache_state(path: &Path) -> Result<CacheState, csv::Error> {
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
