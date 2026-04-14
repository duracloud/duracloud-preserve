use std::collections::BTreeMap;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

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
    pub by_prefix: BTreeMap<String, PrefixStats>,
}

/// File and size counts for a single (top level) prefix
#[derive(Debug, Serialize, Deserialize)]
pub struct PrefixStats {
    pub total_files: u64,
    pub total_size: u64,
}

/// Checksum verification stats
#[derive(Debug, Default, Deserialize)]
pub struct VerificationStats {
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

    pub fn total_objects(&self) -> usize {
        self.matches
            + self.mismatches
            + self.missing_replica
            + self.missing_source
            + self.failed_source
            + self.failed_replication
    }
}

impl Serialize for VerificationStats {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("VerificationStats", 7)?;
        s.serialize_field("total_objects", &self.total_objects())?;
        s.serialize_field("matches", &self.matches)?;
        s.serialize_field("mismatches", &self.mismatches)?;
        s.serialize_field("missing_replica", &self.missing_replica)?;
        s.serialize_field("missing_source", &self.missing_source)?;
        s.serialize_field("failed_source", &self.failed_source)?;
        s.serialize_field("failed_replication", &self.failed_replication)?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_stats_flattened_deserialization() {
        let json = r#"{"bucket":"my-bucket","total_files":5,"total_size":500,"by_prefix":{}}"#;
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
                by_prefix: BTreeMap::new(),
            },
        };

        let json = serde_json::to_string(&bucket_stats).unwrap();
        // flatten means fields appear at top level, not nested under "stats"
        assert!(json.contains("\"bucket\":\"my-bucket\""));
        assert!(json.contains("\"total_files\":5"));
        assert!(!json.contains("\"stats\""));
    }

    #[test]
    fn test_inventory_stats_json_roundtrip() {
        let stats = InventoryStats {
            total_files: 42,
            total_size: 123456,
            by_prefix: BTreeMap::from([(
                "data".to_string(),
                PrefixStats {
                    total_files: 42,
                    total_size: 123456,
                },
            )]),
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: InventoryStats = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total_files, 42);
        assert_eq!(deserialized.total_size, 123456);
        assert_eq!(deserialized.by_prefix.len(), 1);
        assert!(deserialized.by_prefix.contains_key("data"));
    }

    #[test]
    fn test_verification_stats_is_not_ok() {
        let base = VerificationStats {
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

    #[test]
    fn test_verification_stats_is_ok() {
        let stats = VerificationStats {
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
    fn test_verification_stats_total_objects_sums_fields() {
        let stats = VerificationStats {
            matches: 10,
            mismatches: 2,
            missing_replica: 3,
            missing_source: 4,
            failed_source: 5,
            failed_replication: 6,
        };
        assert_eq!(stats.total_objects(), 30);
    }

    #[test]
    fn test_verification_stats_serializes_total_objects() {
        let stats = VerificationStats {
            matches: 10,
            mismatches: 2,
            missing_replica: 3,
            missing_source: 4,
            failed_source: 5,
            failed_replication: 6,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total_objects\":30"));
        assert!(json.contains("\"matches\":10"));
        assert!(json.contains("\"failed_replication\":6"));
    }

    #[test]
    fn test_verification_stats_deserialize_ignores_total_objects() {
        // Incoming total_objects is discarded — recomputed from component fields
        let json = r#"{"total_objects":999,"matches":1,"mismatches":2,"missing_replica":3,"missing_source":4,"failed_source":5,"failed_replication":6}"#;
        let stats: VerificationStats = serde_json::from_str(json).unwrap();
        assert_eq!(stats.total_objects(), 21);
    }
}
