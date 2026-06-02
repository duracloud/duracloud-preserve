# inventory-report

**Type:** Lambda function  
**Trigger:** S3 event (`manifest.json` is created)  
**Dependencies:** None

## Overview

This function processes Parquet-formatted S3 inventory data into a single human-readable CSV manifest per bucket. It also generates storage usage statistics used by the storage report:

- Total number of files and total storage used
- The same, broken down by top-level prefix (folder)

> [!NOTE]
> At least one bucket must exist with files uploaded before this function can run. It has no inventory to process otherwise.

## CLI testing

Run locally against the most recently available S3 inventory for a bucket:

```bash
make run-inventory-report b=digipres-dev1-private p=default
```

- `b=` — Bucket name to process the inventory report for (required)
- `p=` — AWS profile to use (required)

## Remote testing

Staging a remote test requires crafting a specific event payload and uploading matching Parquet files, which adds significant overhead. In practice it is simpler to let the infrastructure run on its normal daily schedule and inspect the logs if the report does not appear.

If the CLI works but the Lambda does not, the most likely cause is an IAM permissions issue.

To stage a full remote test:

1. Craft an event payload that references a `manifest.json`.
2. Upload Parquet inventory files to the location referenced in the `manifest.json`.
3. Upload the `manifest.json` to the path specified in the event payload — this must be within the event notification path (`/manifests`).
4. Ensure the Parquet files contain the correct stack-prefixed bucket name.

## Output

When run successfully there should be four generated files:

- `metadata/0000-00-00-LATEST/manifests/stats/$bucket.csv`
- `metadata/YYYY-MM-DD/manifests/stats/$bucket.csv`
- `reports/0000-00-00-LATEST/manifests/$bucket.csv`
- `reports/YYYY-MM-DD/manifests/$bucket.csv`

To access the latest report you can do:

```bash
aws s3 cp \
    s3://digipres-dev1-managed/reports/0000-00-00-LATEST/manifests/digipres-dev1-private.csv \
    . \
    --profile default
```

## QA testing

Confirm:

- All expected files are available.
- The report contains expected items.
- The stats are accurate.
