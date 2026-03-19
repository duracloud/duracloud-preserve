# storage-report

- Triggered by: scheduled eventbridge event (schedule: weekly)
- Dependencies: inventory-report

## Overview

This function generates a consolidated storage report for a stack (all standard and public buckets) output as a single html file that uses [chart.js](#) for visual representation. The main sections are:

- Aggregated totals for all buckets
- Per bucket totals
- Per bucket / per prefix totals

Like the inventory report the storage report cannot be created without S3 generated inventory being available and at least one inventory report must have been uploaded.

## CLI testing

```bash
make run-storage-report s=digipres-dev1 p=default
```

## Remote testing

```bash
make trigger f=storage-report s=digipres-dev1 p=default
```

## Output

When run successfully there should be four generated files:

- `metadata/latest/storage/stats/$stack.json`
- `metadata/YYYY-MM-DD/storage/stats/$stack.json`
- `reports/latest/storage/$stack.html`
- `reports/YYYY-MM-DD/storage/$stack.html`

