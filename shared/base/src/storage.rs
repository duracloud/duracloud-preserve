use std::collections::BTreeMap;

use askama::Template;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::format_bytes;
use crate::stats::{BucketStats, InventoryStats};

/// Consolidated storage report across all buckets
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageReport {
    #[serde(flatten)]
    pub header: StorageReportHeader,
    #[serde(flatten)]
    pub data: StorageReportData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageReportHeader {
    pub owner: String,
    pub stack_name: String,
    pub generated_at: String,
    pub storage_capacity_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_transfer_out_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_transfer_out_period_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_transfer_out_period_end: Option<String>,
}

/// Aggregated inventory data across all buckets
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageReportData {
    pub total_files: u64,
    pub total_size: u64,
    pub buckets: Vec<BucketStats>,
}

#[derive(Template)]
#[template(path = "storage_report.html")]
struct StorageReportView {
    owner: String,
    stack_name: String,
    generated_at: String,
    total_files: u64,
    total_size_formatted: String,
    bucket_count: usize,
    prefix_count: usize,
    has_capacity: bool,
    capacity_used_pct: String,
    has_data_transfer_out: bool,
    data_transfer_out_formatted: String,
    data_transfer_out_period: String,
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

/// S3 internet egress for the stack over a billing window. Year-to-date as
/// reported by AWS Cost Explorer.
#[derive(Debug, Clone)]
pub struct BillingTransferOut {
    pub bytes: u64,
    pub period_start: String,
    pub period_end: String,
}

impl StorageReport {
    pub fn assemble(
        owner: String,
        stack_name: String,
        storage_capacity_bytes: Option<u64>,
        bucket_stats: BTreeMap<String, InventoryStats>,
        data_transfer_out: Option<BillingTransferOut>,
    ) -> Self {
        let (data_transfer_out_bytes, data_transfer_out_period_start, data_transfer_out_period_end) =
            match data_transfer_out {
                Some(t) => (Some(t.bytes), Some(t.period_start), Some(t.period_end)),
                None => (None, None, None),
            };

        Self {
            header: StorageReportHeader {
                owner,
                stack_name,
                generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                storage_capacity_bytes,
                data_transfer_out_bytes,
                data_transfer_out_period_start,
                data_transfer_out_period_end,
            },
            data: StorageReportData::from_inventory(bucket_stats),
        }
    }

    pub fn to_html(&self) -> Result<String, askama::Error> {
        StorageReportView::from_report(self).render()
    }
}

impl StorageReportData {
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
}

impl StorageReportView {
    fn from_report(report: &StorageReport) -> Self {
        let data = &report.data;
        let header = &report.header;

        let bucket_count = data.buckets.len();
        let prefix_count: usize = data
            .buckets
            .iter()
            .map(|bucket| bucket.stats.by_prefix.len())
            .sum();

        let (has_capacity, capacity_used_pct) = match header.storage_capacity_bytes {
            Some(capacity) if capacity > 0 => (
                true,
                format!(
                    "{} (of {})",
                    format_percent(percent(data.total_size, capacity)),
                    format_bytes(capacity)
                ),
            ),
            _ => (false, String::new()),
        };

        let (has_data_transfer_out, data_transfer_out_formatted, data_transfer_out_period) =
            match header.data_transfer_out_bytes {
                Some(bytes) => (
                    true,
                    format_bytes(bytes),
                    match (
                        header.data_transfer_out_period_start.as_deref(),
                        header.data_transfer_out_period_end.as_deref(),
                    ) {
                        (Some(start), Some(end)) => format!("{start} → {end}"),
                        _ => String::new(),
                    },
                ),
                None => (false, String::new(), String::new()),
            };

        let buckets = data
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
                        total_size_formatted: format_bytes(prefix_stats.total_size),
                        pct_bucket_size: format_percent(percent(
                            prefix_stats.total_size,
                            bucket.stats.total_size,
                        )),
                    })
                    .collect();

                BucketView {
                    name: bucket.bucket.clone(),
                    total_files: bucket.stats.total_files,
                    total_size_formatted: format_bytes(bucket.stats.total_size),
                    pct_total_size: format_percent(percent(
                        bucket.stats.total_size,
                        data.total_size,
                    )),
                    pct_total_files: format_percent(percent(
                        bucket.stats.total_files,
                        data.total_files,
                    )),
                    prefix_count,
                    prefixes,
                }
            })
            .collect::<Vec<_>>();

        let chart_labels = data
            .buckets
            .iter()
            .map(|bucket| bucket.bucket.clone())
            .collect::<Vec<_>>();
        let chart_sizes = data
            .buckets
            .iter()
            .map(|bucket| bucket.stats.total_size)
            .collect::<Vec<_>>();
        let chart_files = data
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
                "labels": data
                    .buckets
                    .iter()
                    .map(|bucket| bucket.bucket.clone())
                    .collect::<Vec<_>>(),
                "values": chart_files,
            })
            .to_string(),
        );

        Self {
            owner: header.owner.clone(),
            stack_name: header.stack_name.clone(),
            generated_at: DateTime::parse_from_rfc3339(&header.generated_at)
                .map(|dt| dt.format("%m/%d/%Y %H:%M:%S UTC").to_string())
                .unwrap_or_else(|_| header.generated_at.clone()),
            total_files: data.total_files,
            total_size_formatted: format_bytes(data.total_size),
            bucket_count,
            prefix_count,
            has_capacity,
            capacity_used_pct,
            has_data_transfer_out,
            data_transfer_out_formatted,
            data_transfer_out_period,
            buckets,
            chart_size_json,
            chart_files_json,
        }
    }
}

fn format_percent(value: f64) -> String {
    format!("{value:.1}%")
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

    fn test_header(storage_capacity_bytes: Option<u64>) -> StorageReportHeader {
        StorageReportHeader {
            owner: "Example Owner".to_string(),
            stack_name: "test-stack".to_string(),
            generated_at: "2026-03-06T00:00:00Z".to_string(),
            storage_capacity_bytes,
            data_transfer_out_bytes: None,
            data_transfer_out_period_start: None,
            data_transfer_out_period_end: None,
        }
    }

    fn test_header_with_transfer_out(
        storage_capacity_bytes: Option<u64>,
        bytes: u64,
    ) -> StorageReportHeader {
        StorageReportHeader {
            owner: "Example Owner".to_string(),
            stack_name: "test-stack".to_string(),
            generated_at: "2026-03-06T00:00:00Z".to_string(),
            storage_capacity_bytes,
            data_transfer_out_bytes: Some(bytes),
            data_transfer_out_period_start: Some("2026-01-01".to_string()),
            data_transfer_out_period_end: Some("2026-05-07".to_string()),
        }
    }

    fn test_report(storage_capacity_bytes: Option<u64>) -> StorageReport {
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

        StorageReport {
            header: test_header(storage_capacity_bytes),
            data: StorageReportData::from_inventory(BTreeMap::from([
                ("alpha-bucket".to_string(), stats_a),
                ("beta-bucket".to_string(), stats_b),
            ])),
        }
    }

    #[test]
    fn test_from_inventory_empty() {
        let data = StorageReportData::from_inventory(BTreeMap::new());
        assert_eq!(data.total_files, 0);
        assert_eq!(data.total_size, 0);
        assert!(data.buckets.is_empty());
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

        let data = StorageReportData::from_inventory(BTreeMap::from([
            ("zebra-bucket".to_string(), stats_a),
            ("alpha-bucket".to_string(), stats_b),
        ]));

        assert_eq!(data.total_files, 30);
        assert_eq!(data.total_size, 13000);
        assert_eq!(data.buckets[0].bucket, "alpha-bucket");
        assert_eq!(data.buckets[1].bucket, "zebra-bucket");
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

        let data =
            StorageReportData::from_inventory(BTreeMap::from([("my-bucket".to_string(), stats)]));

        assert_eq!(data.total_files, 10);
        assert_eq!(data.total_size, 5000);
        assert_eq!(data.buckets.len(), 1);
        assert_eq!(data.buckets[0].bucket, "my-bucket");
        assert_eq!(data.buckets[0].stats.total_files, 10);

        let mut prefixes = data.buckets[0].stats.by_prefix.keys();
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

        let data = StorageReportData::from_inventory(buckets);
        assert_eq!(data.total_files, 500);
        assert_eq!(data.total_size, 5000);
        assert_eq!(data.buckets.len(), 5);
    }

    #[test]
    fn test_to_html_contains_meta_and_sections() {
        let report = test_report(None);
        let html = report.to_html().unwrap();

        assert!(html.contains("<title>Example Owner Storage Report</title>"));
        assert!(html.contains("<h1>Example Owner Storage Report</h1>"));
        assert!(html.contains("Stack: test-stack"));
        assert!(html.contains("Generated: 03/06/2026 00:00:00 UTC"));
        assert!(html.contains("1. Big Picture"));
        assert!(html.contains("2. Per-Bucket Rundown"));
        assert!(html.contains("3. Per-Bucket / Per-Prefix"));
    }

    #[test]
    fn test_to_html_shows_capacity_metric_when_present() {
        let report = test_report(Some(200_000_000));
        let html = report.to_html().unwrap();

        assert!(html.contains("Capacity Used"));
        assert!(html.contains("50.0% (of 200 MB)"));
    }

    #[test]
    fn test_to_html_omits_capacity_metric_when_absent() {
        let report = test_report(None);
        let html = report.to_html().unwrap();

        assert!(!html.contains("Capacity Used"));
    }

    #[test]
    fn test_to_html_shows_data_transfer_out_metric_when_present() {
        let report = StorageReport {
            header: test_header_with_transfer_out(None, 1_500_000_000),
            data: StorageReportData::from_inventory(BTreeMap::new()),
        };
        let html = report.to_html().unwrap();

        let expected_size = format_bytes(1_500_000_000);
        assert!(html.contains("Data Transfer Out (YTD)"));
        assert!(html.contains(&expected_size));
        assert!(html.contains("2026-01-01 → 2026-05-07"));
    }

    #[test]
    fn test_to_html_omits_data_transfer_out_metric_when_absent() {
        let report = test_report(None);
        let html = report.to_html().unwrap();

        assert!(!html.contains("Data Transfer Out"));
    }

    #[test]
    fn test_to_html_empty_report_renders_empty_state() {
        let report = StorageReport {
            header: test_header(None),
            data: StorageReportData::from_inventory(BTreeMap::new()),
        };
        let html = report.to_html().unwrap();

        assert!(html.contains("No bucket stats available."));
        assert!(html.contains("No prefix stats available."));
    }

    #[test]
    fn test_to_html_escapes_bucket_and_prefix_names() {
        let report = StorageReport {
            header: test_header(None),
            data: StorageReportData::from_inventory(BTreeMap::from([(
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
            )])),
        };

        let html = report.to_html().unwrap();

        assert!(html.contains("&#60;bucket&#62;&#38;name"));
        assert!(html.contains("&#60;prefix&#62;&#38;name"));
    }

    #[test]
    fn test_to_html_uses_si_units_not_binary_units() {
        let report = test_report(None);
        let html = report.to_html().unwrap();

        assert!(html.contains("MB") || html.contains("GB") || html.contains("TB"));
        assert!(!html.contains("KiB"));
        assert!(!html.contains("MiB"));
        assert!(!html.contains("GiB"));
    }

    #[test]
    fn test_storage_report_serializes_flat_owner_and_header_fields() {
        let report = test_report(Some(200_000_000));
        let json = serde_json::to_value(&report).unwrap();

        assert_eq!(json["owner"], "Example Owner");
        assert_eq!(json["stack_name"], "test-stack");
        assert_eq!(json["generated_at"], "2026-03-06T00:00:00Z");
        assert_eq!(json["storage_capacity_bytes"], 200_000_000);
        assert_eq!(json["total_files"], 30);
        assert_eq!(json["total_size"], 100_000_000);
        assert!(json.get("header").is_none());
        assert!(json.get("data").is_none());
    }

    #[test]
    fn test_storage_report_omits_transfer_out_fields_from_json_when_absent() {
        let report = test_report(None);
        let json = serde_json::to_value(&report).unwrap();

        assert!(json.get("data_transfer_out_bytes").is_none());
        assert!(json.get("data_transfer_out_period_start").is_none());
        assert!(json.get("data_transfer_out_period_end").is_none());
    }

    #[test]
    fn test_storage_report_includes_transfer_out_fields_in_json_when_present() {
        let report = StorageReport {
            header: test_header_with_transfer_out(None, 1_500_000_000),
            data: StorageReportData::from_inventory(BTreeMap::new()),
        };
        let json = serde_json::to_value(&report).unwrap();

        assert_eq!(json["data_transfer_out_bytes"], 1_500_000_000);
        assert_eq!(json["data_transfer_out_period_start"], "2026-01-01");
        assert_eq!(json["data_transfer_out_period_end"], "2026-05-07");
    }

    #[test]
    fn test_assemble_passes_transfer_out_into_header() {
        let report = StorageReport::assemble(
            "Example Owner".to_string(),
            "test-stack".to_string(),
            Some(200_000_000),
            BTreeMap::new(),
            Some(BillingTransferOut {
                bytes: 7_500_000_000,
                period_start: "2026-01-01".to_string(),
                period_end: "2026-05-07".to_string(),
            }),
        );

        assert_eq!(report.header.data_transfer_out_bytes, Some(7_500_000_000));
        assert_eq!(
            report.header.data_transfer_out_period_start.as_deref(),
            Some("2026-01-01")
        );
        assert_eq!(
            report.header.data_transfer_out_period_end.as_deref(),
            Some("2026-05-07")
        );
    }

    #[test]
    fn test_assemble_leaves_transfer_out_none_when_not_provided() {
        let report = StorageReport::assemble(
            "Example Owner".to_string(),
            "test-stack".to_string(),
            None,
            BTreeMap::new(),
            None,
        );

        assert!(report.header.data_transfer_out_bytes.is_none());
        assert!(report.header.data_transfer_out_period_start.is_none());
        assert!(report.header.data_transfer_out_period_end.is_none());
    }
}
