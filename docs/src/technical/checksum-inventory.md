# checksum-inventory

**Type:** Lambda function  
**Trigger:** S3 event (`.txt` file uploaded under the request bucket's `checksums/` prefix)  
**Dependencies:** [inventory-report](./inventory-report.md) (the manifest CSV must already exist)

## Overview

This function turns an inventory manifest CSV into a checksum inventory: for every object listed in the manifest it issues a HEAD request, records the CRC64NVMe checksum (when present) and a per-object status (`ok`, `not_found`, `missing_checksum`, or `error`), and uploads the result as a CSV under the managed bucket's `reports/*/checksums/` path.

The trigger file's name (with extension stripped) identifies the source bucket whose inventory should be processed. For example, uploading `checksums/digipres-dev1-private.txt` to the request bucket processes the inventory for `digipres-dev1-private`.

The workflow is:

1. A `.txt` file named `<bucket>.txt` is uploaded to `s3://${stack}-request/checksums/`.
2. The Lambda function is triggered by the upload event.
3. The function parses the bucket name from the trigger file name.
4. The function checks that the matching inventory manifest exists at `s3://${stack}-managed/reports/latest/manifests/<bucket>.csv`.
5. If present, the inventory is processed and the resulting checksum CSV is uploaded to the managed bucket.
6. The trigger file is deleted on success so a re-upload is required to re-trigger.

## CLI testing

Run locally against an existing manifest for a bucket:

```bash
cargo run -p dcp -- checksum-inventory --bucket digipres-dev1-private
```

- `--bucket` — Bucket name to process the checksum inventory for (required)

The CLI fails fast with `Inventory report not found` if no manifest is available; run [inventory-report](./inventory-report.md) first.

## Remote testing

Upload a trigger file to the request bucket's `checksums/` prefix:

```bash
make upload b=digipres-dev1-request d=checksums f=files/digipres-dev1-private.txt p=default
```

- `b=` — the name of the S3 request bucket (typically `${stack}-request`)
- `d=` — the S3 directory (path) to upload into (must be `checksums`)
- `f=` — path to a local trigger file; its basename (without extension) must be the source bucket name
- `p=` — the AWS profile to use

The trigger file's contents are not read — only its name is used.

## Output

When run successfully there should be two generated files:

- `reports/latest/checksums/<bucket>_checksum-inventory.csv`
- `reports/YYYY-MM-DD/checksums/<bucket>_checksum-inventory.csv`

To access the latest report you can do:

```bash
aws s3 cp \
    s3://digipres-dev1-managed/reports/latest/checksums/digipres-dev1-private_checksum-inventory.csv \
    . \
    --profile default
```

## QA testing

Aside from the happy path, here are variations to try:

- Trigger file uploaded for a bucket that has no inventory manifest yet — the lambda should error with "Inventory report not found".
- Trigger file with a name that does not parse to a valid bucket (e.g., no extension) — the lambda should fail before doing any work.
- Trigger file uploaded outside the `checksums/` prefix — the lambda should not be invoked.
