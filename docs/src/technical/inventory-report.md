# inventory-report

Triggered by: s3 event (manifest.json)
Dependencies: None

1. Processes parquet formatted inventory into a single human readable csv.
2. Generates storage usage stats including by (top level) prefix.
