# storage-report

**Type:** Lambda function  
**Trigger:** Scheduled EventBridge event (weekly)  
**Dependencies:** inventory-report

## Overview

This Lambda function generates a consolidated storage report for a stack, displaying storage usage across all standard and public buckets. The report is output as a single interactive HTML file using [Chart.js](https://www.chartjs.org/) for visualizations.

### Report sections

- **Aggregated totals** — Storage usage across all buckets in the stack
- **Per bucket totals** — Storage usage broken down by individual bucket
- **Per bucket / per prefix totals** — Storage usage by prefix within each bucket

### Prerequisites

The storage report requires S3 inventory data to be available. Before running this function:
1. S3 inventory must be enabled for the buckets
2. At least one inventory report must have been generated and uploaded
3. The `inventory-report` function must have completed successfully

## CLI testing

Generate a storage report for a specific stack:

```bash
make run-storage-report s=digipres-dev1 p=default
```

**Parameters:**
- `s=` — Stack name (required)
- `p=` — AWS profile (required)

## Remote trigger

```bash
make trigger f=storage-report s=digipres-dev1 p=default
```

**Parameters:**
- `f=` — Function name (storage-report)
- `s=` — Stack name (required)
- `p=` — AWS profile (required)

### Scheduled execution

Automatically triggered weekly by EventBridge.

## Output

When successful, four files are generated:

### Statistics (JSON format)

- `metadata/0000-00-00-LATEST/storage/stats/{stack}.json` — Latest version
- `metadata/YYYY-MM-DD/storage/stats/{stack}.json` — Date-stamped archive

Contains raw storage metrics for programmatic access.

### Report (HTML format)

- `reports/0000-00-00-LATEST/storage/{stack}.html` — Latest version
- `reports/YYYY-MM-DD/storage/{stack}.html` — Date-stamped archive

Interactive HTML report with Chart.js visualizations for viewing in a browser.