use archive_it_client::models::wasapi::{Checksums, WasapiFile};
use serde::{Deserialize, Serialize};

const LOCATIONS_DELIMITER: char = ';';

/// CSV row for the Archive-It inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryRow {
    pub collection_id: u64,
    pub collection_name: String,
    pub account_id: u64,
    pub filename: String,
    pub filetype: String,
    pub size_bytes: u64,
    pub crawl_id: Option<u64>,
    pub crawl_time: Option<String>,
    pub crawl_start: Option<String>,
    pub store_time: String,
    pub sha1: Option<String>,
    pub md5: Option<String>,
    pub primary_location: String,
    /// Semicolon-delimited list of all WASAPI-reported locations.
    pub all_locations: String,
}

impl InventoryRow {
    pub fn from_wasapi(
        file: &WasapiFile,
        collection_name: impl Into<String>,
        primary_location: impl Into<String>,
    ) -> Self {
        Self {
            collection_id: file.collection,
            collection_name: collection_name.into(),
            account_id: file.account,
            filename: file.filename.clone(),
            filetype: file.filetype.clone(),
            size_bytes: file.size,
            crawl_id: file.crawl,
            crawl_time: file.crawl_time.clone(),
            crawl_start: file.crawl_start.clone(),
            store_time: file.store_time.clone(),
            sha1: file.checksums.sha1.clone(),
            md5: file.checksums.md5.clone(),
            primary_location: primary_location.into(),
            all_locations: file.locations.join(&LOCATIONS_DELIMITER.to_string()),
        }
    }
}

impl From<&InventoryRow> for WasapiFile {
    fn from(row: &InventoryRow) -> Self {
        WasapiFile {
            filename: row.filename.clone(),
            filetype: row.filetype.clone(),
            checksums: Checksums {
                sha1: row.sha1.clone(),
                md5: row.md5.clone(),
            },
            account: row.account_id,
            size: row.size_bytes,
            collection: row.collection_id,
            crawl: row.crawl_id,
            crawl_time: row.crawl_time.clone(),
            crawl_start: row.crawl_start.clone(),
            store_time: row.store_time.clone(),
            locations: row
                .all_locations
                .split(LOCATIONS_DELIMITER)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use csv::{ReaderBuilder, WriterBuilder};

    fn populated_row() -> InventoryRow {
        InventoryRow {
            collection_id: 4472,
            collection_name: "Test Collection".into(),
            account_id: 1,
            filename: "ARCHIVEIT-1.warc.gz".into(),
            filetype: "warc".into(),
            size_bytes: 12345,
            crawl_id: Some(7777),
            crawl_time: Some("2025-01-01T00:00:00Z".into()),
            crawl_start: Some("2024-12-31T23:00:00Z".into()),
            store_time: "2025-01-02T00:00:00Z".into(),
            sha1: Some("sha1hex".into()),
            md5: Some("md5hex".into()),
            primary_location: "https://example.invalid/a.warc.gz".into(),
            all_locations: "https://example.invalid/a.warc.gz;https://example.invalid/b.warc.gz"
                .into(),
        }
    }

    fn sparse_row() -> InventoryRow {
        InventoryRow {
            collection_id: 4472,
            collection_name: "Test Collection".into(),
            account_id: 1,
            filename: "ARCHIVEIT-2.warc.gz".into(),
            filetype: "warc".into(),
            size_bytes: 0,
            crawl_id: None,
            crawl_time: None,
            crawl_start: None,
            store_time: "2025-01-02T00:00:00Z".into(),
            sha1: None,
            md5: None,
            primary_location: String::new(),
            all_locations: String::new(),
        }
    }

    fn csv_round_trip(row: &InventoryRow) -> InventoryRow {
        let mut buf = Vec::new();
        {
            let mut wtr = WriterBuilder::new().has_headers(true).from_writer(&mut buf);
            wtr.serialize(row).expect("serialize");
            wtr.flush().expect("flush");
        }
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(buf.as_slice());
        rdr.deserialize::<InventoryRow>()
            .next()
            .expect("at least one row")
            .expect("deserialize")
    }

    #[test]
    fn csv_round_trip_preserves_populated_row() {
        let row = populated_row();
        assert_eq!(csv_round_trip(&row), row);
    }

    /// Critical: confirms the `csv` crate handles `Option<u64>` (`crawl_id`)
    /// and `Option<String>` cells natively — empty cell deserializes to `None`
    #[test]
    fn csv_round_trip_preserves_all_none_optionals() {
        let row = sparse_row();
        assert_eq!(csv_round_trip(&row), row);
    }

    #[test]
    fn wasapi_file_round_trips_through_inventory_row() {
        let original = WasapiFile {
            filename: "ARCHIVEIT-1.warc.gz".into(),
            filetype: "warc".into(),
            checksums: Checksums {
                sha1: Some("sha1hex".into()),
                md5: None,
            },
            account: 1,
            size: 12345,
            collection: 4472,
            crawl: Some(7777),
            crawl_time: Some("2025-01-01T00:00:00Z".into()),
            crawl_start: None,
            store_time: "2025-01-02T00:00:00Z".into(),
            locations: vec![
                "https://example.invalid/a.warc.gz".into(),
                "https://example.invalid/b.warc.gz".into(),
            ],
        };

        let row = InventoryRow::from_wasapi(&original, "Test Collection", &original.locations[0]);
        let recovered: WasapiFile = (&row).into();

        assert_eq!(recovered.filename, original.filename);
        assert_eq!(recovered.filetype, original.filetype);
        assert_eq!(recovered.checksums.sha1, original.checksums.sha1);
        assert_eq!(recovered.checksums.md5, original.checksums.md5);
        assert_eq!(recovered.account, original.account);
        assert_eq!(recovered.size, original.size);
        assert_eq!(recovered.collection, original.collection);
        assert_eq!(recovered.crawl, original.crawl);
        assert_eq!(recovered.crawl_time, original.crawl_time);
        assert_eq!(recovered.crawl_start, original.crawl_start);
        assert_eq!(recovered.store_time, original.store_time);
        assert_eq!(recovered.locations, original.locations);
    }
}
