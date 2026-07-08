use std::fs::OpenOptions;
use std::path::PathBuf;
use std::pin::pin;

use archive_it_client::wasapi::DEFAULT_WEBDATA_PAGE_SIZE;
use archive_it_client::{Config, PartnerClient, WasapiClient, WebdataQuery};
use csv::{Terminator, WriterBuilder};
use futures::TryStreamExt;

use crate::errors::ArchiveItError;
use crate::inventory::{CacheState, InventoryRow, read_cache_state};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryStats {
    pub collection_count: u64,
    pub collection_skipped: u64,
    pub warc_count: u64,
    pub cache_hit_count: u64,
}

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub username: String,
    pub password: String,
    /// Optional allow-list header sent on every Archive-It request.
    pub header: Option<(String, String)>,
    pub output: PathBuf,
}

pub async fn perform(args: &PerformArgs) -> Result<InventoryStats, ArchiveItError> {
    let partner = PartnerClient::with_config(
        args.username.clone(),
        args.password.clone(),
        super::with_header(Config::api(), args.header.as_ref()),
    )?;
    let wasapi = WasapiClient::with_config(
        args.username.clone(),
        args.password.clone(),
        super::with_header(Config::wasapi(), args.header.as_ref()),
    )?;

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
    let mut stats = InventoryStats::default();

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
            stats.collection_skipped += 1;
            stats.cache_hit_count += cached;
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

        stats.collection_count += 1;
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

        stats.warc_count += written_in_collection;
        stats.cache_hit_count += cached_in_collection;
    }

    writer.flush()?;

    Ok(stats)
}
