use serde::{Deserialize, Serialize};

use crate::stats::{BucketStats, InventoryStats};

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

    fn _to_html(&self) -> String {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::stats::PrefixStats;

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
}
