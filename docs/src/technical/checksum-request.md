# checksum-request

> **Trigger:** S3 event (`.txt` file uploaded under the request bucket's `checksums/` prefix)  
> **Dependencies:** [`inventory-report`](./inventory-report.md) — the manifest CSV must already exist before running this

## Overview

`checksum-request` turns an inventory manifest CSV into a checksum inventory. For every object listed in the manifest, it issues a HEAD request, records the CRC64NVMe checksum (when present), and assigns a per-object status of `ok`, `not_found`, `missing_checksum`, or `error`. The result is uploaded as a CSV to the managed bucket under `reports/*/checksums/`.

The trigger file's name (minus the extension) identifies which bucket's inventory to process. For example, uploading `checksums/digipres-dev1-private.txt` processes the inventory for `digipres-dev1-private`.

**Workflow:**

1. A `.txt` file named `<bucket>.txt` is uploaded to `s3://${stack}-request/checksums/`
2. The Lambda function is triggered by the upload event
3. The bucket name is parsed from the trigger filename
4. The function checks for a matching inventory manifest at `s3://${stack}-managed/reports/latest/manifests/<bucket>.csv`
5. If found, the inventory is processed and the checksum CSV is uploaded to the managed bucket
6. The trigger file is deleted on success — re-upload to re-trigger

## CLI testing

Run locally against an existing manifest:

```bash
cargo run -p dcp -- checksum-request --bucket digipres-dev1-private
```

| Flag       | Description                                          |
| ---------- | ---------------------------------------------------- |
| `--bucket` | Bucket name to process the checksum inventory for (required) |

> [!IMPORTANT]
> If no manifest exists for the bucket, the CLI will fail with `Inventory report not found`. Run [`inventory-report`](./inventory-report.md) first.

## Remote testing

Upload a trigger file to the request bucket's `checksums/` prefix:

```bash
make upload b=digipres-dev1-request d=checksums f=files/digipres-dev1-private.txt p=default
```

| Flag | Description                                                                             |
| ---- | --------------------------------------------------------------------------------------- |
| `b=` | The S3 request bucket (typically `${stack}-request`)                                    |
| `d=` | The S3 path to upload into — must be `checksums`                                        |
| `f=` | Path to a local trigger file; its basename (without extension) must be the bucket name  |
| `p=` | AWS profile                                                                             |

> [!NOTE]
> The trigger file's contents are not read — only its name matters.

## Output

A successful run writes two files to the managed bucket:

- `reports/latest/checksums/<bucket>_checksum-inventory.csv`
- `reports/YYYY-MM-DD/checksums/<bucket>_checksum-inventory.csv`

To download the latest report:

```bash
aws s3 cp \
    s3://digipres-dev1-managed/reports/latest/checksums/digipres-dev1-private_checksum-inventory.csv \
    . \
    --profile default
```

## QA testing

In addition to the happy path, test these edge cases:

| Scenario | Expected behaviour |
| --- | --- |
| Trigger file uploaded with no matching inventory manifest | Fails with `Inventory report not found` |
| Trigger filename does not parse to a valid bucket (e.g. no extension) | Fails before doing any work |
| Trigger file uploaded outside the `checksums/` prefix | Lambda is not invoked |
