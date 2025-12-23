use serde::Deserialize;

#[derive(Deserialize)]
pub struct InventoryManifest {
    pub sourceBucket: String,
    pub destinationBucket: String,
    pub version: String,
    pub creationTimestamp: String,
    pub fileFormat: String,
    pub fileSchema: String,
    pub files: Vec<InventoryFileEntry>,
}

#[derive(Deserialize)]
pub struct InventoryFileEntry {
    pub key: String,
    pub size: u64,
    pub MD5checksum: String,
}
