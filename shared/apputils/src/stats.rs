use serde::{Deserialize, Serialize};

/// Per-bucket stats — wraps InventoryStats with a bucket name
#[derive(Debug, Serialize, Deserialize)]
pub struct BucketStats {
    pub bucket: String,
    #[serde(flatten)]
    pub stats: InventoryStats,
}

/// Inventory stats (bucketless payload — stats for any set of inventory rows)
#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryStats {
    pub total_files: u64,
    pub total_size: u64,
    pub by_prefix: Vec<PrefixStats>,
}

/// Inventory stats by (top level) prefix
#[derive(Debug, Serialize, Deserialize)]
pub struct PrefixStats {
    pub prefix: String,
    pub total_files: u64,
    pub total_size: u64,
}

/// Checksum verification stats
#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationStats {
    pub total_objects: usize,
    pub matches: usize,
    pub mismatches: usize,
    pub missing_replica: usize,
    pub missing_source: usize,
    pub failed_source: usize,
    pub failed_replication: usize,
}

/// Consolidated storage report across all buckets
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageReport {
    pub total_files: u64,
    pub total_size: u64,
    pub buckets: Vec<BucketStats>,
}

impl StorageReport {
    pub fn from_inventory(mut bucket_stats: Vec<(String, InventoryStats)>) -> Self {
        bucket_stats.sort_by(|(a, _), (b, _)| a.cmp(b));

        let total_files: u64 = bucket_stats.iter().map(|(_, s)| s.total_files).sum();
        let total_size: u64 = bucket_stats.iter().map(|(_, s)| s.total_size).sum();

        let buckets = bucket_stats
            .into_iter()
            .map(|(bucket, mut stats)| {
                stats.by_prefix.sort_by(|a, b| a.prefix.cmp(&b.prefix));
                BucketStats { bucket, stats }
            })
            .collect();

        Self {
            total_files,
            total_size,
            buckets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_inventory_empty() {
        let report = StorageReport::from_inventory(vec![]);
        assert_eq!(report.total_files, 0);
        assert_eq!(report.total_size, 0);
        assert!(report.buckets.is_empty());
    }

    #[test]
    fn test_from_inventory_single_bucket() {
        let stats = InventoryStats {
            total_files: 10,
            total_size: 5000,
            by_prefix: vec![
                PrefixStats {
                    prefix: "images".to_string(),
                    total_files: 7,
                    total_size: 3500,
                },
                PrefixStats {
                    prefix: "docs".to_string(),
                    total_files: 3,
                    total_size: 1500,
                },
            ],
        };

        let report = StorageReport::from_inventory(vec![("my-bucket".to_string(), stats)]);

        assert_eq!(report.total_files, 10);
        assert_eq!(report.total_size, 5000);
        assert_eq!(report.buckets.len(), 1);
        assert_eq!(report.buckets[0].bucket, "my-bucket");
        assert_eq!(report.buckets[0].stats.total_files, 10);
        // prefixes should be sorted
        assert_eq!(report.buckets[0].stats.by_prefix[0].prefix, "docs");
        assert_eq!(report.buckets[0].stats.by_prefix[1].prefix, "images");
    }

    #[test]
    fn test_from_inventory_multiple_buckets() {
        let stats_a = InventoryStats {
            total_files: 10,
            total_size: 5000,
            by_prefix: vec![],
        };
        let stats_b = InventoryStats {
            total_files: 20,
            total_size: 8000,
            by_prefix: vec![],
        };

        let report = StorageReport::from_inventory(vec![
            ("zebra-bucket".to_string(), stats_a),
            ("alpha-bucket".to_string(), stats_b),
        ]);

        assert_eq!(report.total_files, 30);
        assert_eq!(report.total_size, 13000);
        // buckets should be sorted by name
        assert_eq!(report.buckets[0].bucket, "alpha-bucket");
        assert_eq!(report.buckets[1].bucket, "zebra-bucket");
    }

    #[test]
    fn test_from_inventory_sums_totals() {
        let buckets: Vec<(String, InventoryStats)> = (0..5)
            .map(|i| {
                (
                    format!("bucket-{}", i),
                    InventoryStats {
                        total_files: 100,
                        total_size: 1000,
                        by_prefix: vec![],
                    },
                )
            })
            .collect();

        let report = StorageReport::from_inventory(buckets);
        assert_eq!(report.total_files, 500);
        assert_eq!(report.total_size, 5000);
        assert_eq!(report.buckets.len(), 5);
    }

    #[test]
    fn test_inventory_stats_json_roundtrip() {
        let stats = InventoryStats {
            total_files: 42,
            total_size: 123456,
            by_prefix: vec![PrefixStats {
                prefix: "data".to_string(),
                total_files: 42,
                total_size: 123456,
            }],
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: InventoryStats = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total_files, 42);
        assert_eq!(deserialized.total_size, 123456);
        assert_eq!(deserialized.by_prefix.len(), 1);
        assert_eq!(deserialized.by_prefix[0].prefix, "data");
    }

    #[test]
    fn test_bucket_stats_flattened_deserialization() {
        let json = r#"{"bucket":"my-bucket","total_files":5,"total_size":500,"by_prefix":[]}"#;
        let bucket_stats: BucketStats = serde_json::from_str(json).unwrap();

        assert_eq!(bucket_stats.bucket, "my-bucket");
        assert_eq!(bucket_stats.stats.total_files, 5);
        assert_eq!(bucket_stats.stats.total_size, 500);
        assert!(bucket_stats.stats.by_prefix.is_empty());
    }

    #[test]
    fn test_bucket_stats_flattened_serialization() {
        let bucket_stats = BucketStats {
            bucket: "my-bucket".to_string(),
            stats: InventoryStats {
                total_files: 5,
                total_size: 500,
                by_prefix: vec![],
            },
        };

        let json = serde_json::to_string(&bucket_stats).unwrap();
        // flatten means fields appear at top level, not nested under "stats"
        assert!(json.contains("\"bucket\":\"my-bucket\""));
        assert!(json.contains("\"total_files\":5"));
        assert!(!json.contains("\"stats\""));
    }
}
