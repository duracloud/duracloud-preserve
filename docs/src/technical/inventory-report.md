# inventory-report

- Lambda triggered by: s3 event (`manifest.json` is created)
- Dependencies: None

## Overview

This function processes parquet formatted inventory into a single human
readable csv and it generates storage usage stats that are used by the
storage report:

- Total number of files and total storage used
- The same, but grouped by top level prefix (folder)

For the cli or remote testing to work then at least one bucket must have
been created with some files uploaded into it. Inventory report generation
cannot do work without an S3 generated inventory to process.

## CLI testing

Locally the cli will attempt to use the most recently available inventory:

```bash
make run-inventory-report b=digipress-dev1-private p=default
```

- `b=` bucket name to process the inventory report for.

## Remote testing

Because of the opinionated structure required to stage things for remote
testing, it is simpler to allow the infrastructure to do its work and use
the logs to identify any issues that haven't been surfaced by the cli.

Because inventory reports are generated daily you can just wait for
the next one to appear, and troubleshoot if it does not.

Generally speaking if the cli works and the Lambda does not then the usual
culprit will be permissions.

If you want to go the extra mile to stage things here are the steps:

- craft an event payload that refers to a `manifest.json`
- upload parquet files to the inventory location/s pointed to in the `manifest.json`
- upload the `manifest.json` to the location pointed to in the payload
  - this location should be within the event notification path (`/manifests`)

For this to be realistic you'll also need to ensure that the appropriate
stack name appears for bucket name in the inventory parquet files.

## Output

When run successfully there should be four generated files:

- `metadata/latest/stats/$bucket.csv`
- `metadata/YYYY-MM-DD/stats/$bucket.csv`
- `reports/latest/manifests/$bucket.csv`
- `reports/YYYY-MM-DD/manifests/$bucket.csv`

To access the latest report you can do:

```bash
aws s3 cp \
    s3://digipress-dev1-managed/reports/latest/manifests/digipress-dev1-private.csv \
    . \
    --profile default
```

## QA testing

Confirm:

- All expected files are available.
- The report contains expected items.
- The stats are accurate.
