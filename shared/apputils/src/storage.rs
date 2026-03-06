use std::collections::BTreeMap;

use askama::Template;
use humansize::{DECIMAL, format_size};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::stats::{BucketStats, InventoryStats};

#[derive(Debug, Clone)]
pub struct StorageReportMeta {
    pub stack_name: String,
    pub generated_at: String,
    pub storage_capacity_bytes: Option<u64>,
}

/// Consolidated storage report across all buckets
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageReport {
    pub total_files: u64,
    pub total_size: u64,
    pub buckets: Vec<BucketStats>,
}

#[derive(Template)]
#[template(path = "storage_report.html")]
struct StorageReportView {
    stack_name: String,
    generated_at: String,
    total_files: u64,
    total_size_formatted: String,
    bucket_count: usize,
    prefix_count: usize,
    has_capacity: bool,
    capacity_used_pct: String,
    buckets: Vec<BucketView>,
    chart_size_json: String,
    chart_files_json: String,
}

#[derive(Debug)]
struct BucketView {
    name: String,
    total_files: u64,
    total_size_formatted: String,
    pct_total_size: String,
    pct_total_files: String,
    prefix_count: usize,
    prefixes: Vec<PrefixView>,
}

#[derive(Debug)]
struct PrefixView {
    name: String,
    total_files: u64,
    total_size_formatted: String,
    pct_bucket_size: String,
}

impl StorageReport {
    pub fn from_inventory(bucket_stats: BTreeMap<String, InventoryStats>) -> Self {
        let total_files: u64 = bucket_stats.values().map(|s| s.total_files).sum();
        let total_size: u64 = bucket_stats.values().map(|s| s.total_size).sum();

        let buckets = bucket_stats
            .into_iter()
            .map(|(bucket, stats)| BucketStats { bucket, stats })
            .collect();

        Self {
            total_files,
            total_size,
            buckets,
        }
    }

    pub fn to_html(&self, meta: StorageReportMeta) -> Result<String, askama::Error> {
        StorageReportView::from_report(self, meta).render()
    }
}

impl StorageReportView {
    fn from_report(report: &StorageReport, meta: StorageReportMeta) -> Self {
        let bucket_count = report.buckets.len();
        let prefix_count: usize = report
            .buckets
            .iter()
            .map(|bucket| bucket.stats.by_prefix.len())
            .sum();

        let (has_capacity, capacity_used_pct) = match meta.storage_capacity_bytes {
            Some(capacity) if capacity > 0 => (
                true,
                format!(
                    "{} (of {})",
                    format_percent(percent(report.total_size, capacity)),
                    format_decimal_bytes(capacity)
                ),
            ),
            _ => (false, String::new()),
        };

        let buckets = report
            .buckets
            .iter()
            .map(|bucket| {
                let prefix_count = bucket.stats.by_prefix.len();
                let prefixes = bucket
                    .stats
                    .by_prefix
                    .iter()
                    .map(|(prefix_name, prefix_stats)| PrefixView {
                        name: prefix_name.clone(),
                        total_files: prefix_stats.total_files,
                        total_size_formatted: format_decimal_bytes(prefix_stats.total_size),
                        pct_bucket_size: format_percent(percent(
                            prefix_stats.total_size,
                            bucket.stats.total_size,
                        )),
                    })
                    .collect();

                BucketView {
                    name: bucket.bucket.clone(),
                    total_files: bucket.stats.total_files,
                    total_size_formatted: format_decimal_bytes(bucket.stats.total_size),
                    pct_total_size: format_percent(percent(
                        bucket.stats.total_size,
                        report.total_size,
                    )),
                    pct_total_files: format_percent(percent(
                        bucket.stats.total_files,
                        report.total_files,
                    )),
                    prefix_count,
                    prefixes,
                }
            })
            .collect::<Vec<_>>();

        let chart_labels = report
            .buckets
            .iter()
            .map(|bucket| bucket.bucket.clone())
            .collect::<Vec<_>>();
        let chart_sizes = report
            .buckets
            .iter()
            .map(|bucket| bucket.stats.total_size)
            .collect::<Vec<_>>();
        let chart_files = report
            .buckets
            .iter()
            .map(|bucket| bucket.stats.total_files)
            .collect::<Vec<_>>();

        let chart_size_json = script_safe_json(
            json!({
                "labels": chart_labels,
                "values": chart_sizes,
            })
            .to_string(),
        );

        let chart_files_json = script_safe_json(
            json!({
                "labels": report
                    .buckets
                    .iter()
                    .map(|bucket| bucket.bucket.clone())
                    .collect::<Vec<_>>(),
                "values": chart_files,
            })
            .to_string(),
        );

        Self {
            stack_name: meta.stack_name,
            generated_at: meta.generated_at,
            total_files: report.total_files,
            total_size_formatted: format_decimal_bytes(report.total_size),
            bucket_count,
            prefix_count,
            has_capacity,
            capacity_used_pct,
            buckets,
            chart_size_json,
            chart_files_json,
        }
    }
}

fn format_percent(value: f64) -> String {
    format!("{value:.1}%")
}

fn format_decimal_bytes(value: u64) -> String {
    format_size(value, DECIMAL)
}

fn percent(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }

    (numerator as f64 / denominator as f64) * 100.0
}

fn script_safe_json(value: String) -> String {
    value.replace("</", "<\\/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::PrefixStats;

    fn test_meta(storage_capacity_bytes: Option<u64>) -> StorageReportMeta {
        StorageReportMeta {
            stack_name: "test-stack".to_string(),
            generated_at: "2026-03-06T00:00:00Z".to_string(),
            storage_capacity_bytes,
        }
    }

    fn test_report() -> StorageReport {
        let stats_a = InventoryStats {
            total_files: 10,
            total_size: 60_000_000,
            by_prefix: BTreeMap::from([
                (
                    "docs".to_string(),
                    PrefixStats {
                        total_files: 6,
                        total_size: 45_000_000,
                    },
                ),
                (
                    "images".to_string(),
                    PrefixStats {
                        total_files: 4,
                        total_size: 15_000_000,
                    },
                ),
            ]),
        };

        let stats_b = InventoryStats {
            total_files: 20,
            total_size: 40_000_000,
            by_prefix: BTreeMap::from([(
                "archive".to_string(),
                PrefixStats {
                    total_files: 20,
                    total_size: 40_000_000,
                },
            )]),
        };

        StorageReport::from_inventory(BTreeMap::from([
            ("alpha-bucket".to_string(), stats_a),
            ("beta-bucket".to_string(), stats_b),
        ]))
    }

    #[test]
    fn test_from_inventory_empty() {
        let report = StorageReport::from_inventory(BTreeMap::new());
        assert_eq!(report.total_files, 0);
        assert_eq!(report.total_size, 0);
        assert!(report.buckets.is_empty());
    }

    #[test]
    fn test_from_inventory_multiple_buckets() {
        let stats_a = InventoryStats {
            total_files: 10,
            total_size: 5000,
            by_prefix: BTreeMap::new(),
        };
        let stats_b = InventoryStats {
            total_files: 20,
            total_size: 8000,
            by_prefix: BTreeMap::new(),
        };

        let report = StorageReport::from_inventory(BTreeMap::from([
            ("zebra-bucket".to_string(), stats_a),
            ("alpha-bucket".to_string(), stats_b),
        ]));

        assert_eq!(report.total_files, 30);
        assert_eq!(report.total_size, 13000);
        assert_eq!(report.buckets[0].bucket, "alpha-bucket");
        assert_eq!(report.buckets[1].bucket, "zebra-bucket");
    }

    #[test]
    fn test_from_inventory_single_bucket() {
        let stats = InventoryStats {
            total_files: 10,
            total_size: 5000,
            by_prefix: BTreeMap::from([
                (
                    "images".to_string(),
                    PrefixStats {
                        total_files: 7,
                        total_size: 3500,
                    },
                ),
                (
                    "docs".to_string(),
                    PrefixStats {
                        total_files: 3,
                        total_size: 1500,
                    },
                ),
            ]),
        };

        let report =
            StorageReport::from_inventory(BTreeMap::from([("my-bucket".to_string(), stats)]));

        assert_eq!(report.total_files, 10);
        assert_eq!(report.total_size, 5000);
        assert_eq!(report.buckets.len(), 1);
        assert_eq!(report.buckets[0].bucket, "my-bucket");
        assert_eq!(report.buckets[0].stats.total_files, 10);

        let mut prefixes = report.buckets[0].stats.by_prefix.keys();
        assert_eq!(prefixes.next().unwrap(), "docs");
        assert_eq!(prefixes.next().unwrap(), "images");
    }

    #[test]
    fn test_from_inventory_sums_totals() {
        let buckets: BTreeMap<String, InventoryStats> = (0..5)
            .map(|i| {
                (
                    format!("bucket-{}", i),
                    InventoryStats {
                        total_files: 100,
                        total_size: 1000,
                        by_prefix: BTreeMap::new(),
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
    fn test_to_html_contains_meta_and_sections() {
        let report = test_report();
        let html = report.to_html(test_meta(None)).unwrap();

        assert!(html.contains("Stack: test-stack"));
        assert!(html.contains("Generated: 2026-03-06T00:00:00Z"));
        assert!(html.contains("1. Big Picture"));
        assert!(html.contains("2. Per-Bucket Rundown"));
        assert!(html.contains("3. Per-Bucket / Per-Prefix"));
    }

    #[test]
    fn test_to_html_shows_capacity_kpi_when_present() {
        let report = test_report();
        let html = report.to_html(test_meta(Some(200_000_000))).unwrap();

        assert!(html.contains("Capacity Used"));
        assert!(html.contains("50.0% (of 200 MB)"));
    }

    #[test]
    fn test_to_html_omits_capacity_kpi_when_absent() {
        let report = test_report();
        let html = report.to_html(test_meta(None)).unwrap();

        assert!(!html.contains("Capacity Used"));
    }

    #[test]
    fn test_to_html_empty_report_renders_empty_state() {
        let report = StorageReport::from_inventory(BTreeMap::new());
        let html = report.to_html(test_meta(None)).unwrap();

        assert!(html.contains("No bucket stats available."));
        assert!(html.contains("No prefix stats available."));
    }

    #[test]
    fn test_to_html_escapes_bucket_and_prefix_names() {
        let report = StorageReport::from_inventory(BTreeMap::from([(
            "<bucket>&name".to_string(),
            InventoryStats {
                total_files: 1,
                total_size: 1,
                by_prefix: BTreeMap::from([(
                    "<prefix>&name".to_string(),
                    PrefixStats {
                        total_files: 1,
                        total_size: 1,
                    },
                )]),
            },
        )]));

        let html = report.to_html(test_meta(None)).unwrap();

        assert!(html.contains("&lt;bucket&gt;&amp;name"));
        assert!(html.contains("&lt;prefix&gt;&amp;name"));
    }

    #[test]
    fn test_to_html_uses_si_units_not_binary_units() {
        let report = test_report();
        let html = report.to_html(test_meta(None)).unwrap();

        assert!(html.contains("MB") || html.contains("GB") || html.contains("TB"));
        assert!(!html.contains("KiB"));
        assert!(!html.contains("MiB"));
        assert!(!html.contains("GiB"));
    }
}
