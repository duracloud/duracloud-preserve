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

impl VerificationStats {
    // Do not treat missing_replica as an error due to potential replication lag
    pub fn is_ok(&self) -> bool {
        self.mismatches == 0
            && self.missing_source == 0
            && self.failed_source == 0
            && self.failed_replication == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_verification_stats_is_ok() {
        let stats = VerificationStats {
            total_objects: 100,
            matches: 95,
            mismatches: 0,
            missing_replica: 5,
            missing_source: 0,
            failed_source: 0,
            failed_replication: 0,
        };
        // missing_replica alone should not cause failure
        assert!(stats.is_ok());
    }

    #[test]
    fn test_verification_stats_is_not_ok() {
        let base = VerificationStats {
            total_objects: 100,
            matches: 99,
            mismatches: 0,
            missing_replica: 0,
            missing_source: 0,
            failed_source: 0,
            failed_replication: 0,
        };

        let cases = [
            VerificationStats {
                mismatches: 1,
                ..base
            },
            VerificationStats {
                missing_source: 1,
                ..base
            },
            VerificationStats {
                failed_source: 1,
                ..base
            },
            VerificationStats {
                failed_replication: 1,
                ..base
            },
        ];

        for stats in &cases {
            assert!(!stats.is_ok());
        }
    }
}
