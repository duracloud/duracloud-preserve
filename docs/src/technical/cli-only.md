# CLI-only features

Beyond core functionality, the CLI provides features for data validation, maintenance, and bucket management. These commands are only available through the command-line interface:

- **Computing a checksum** — Trigger S3 batch checksum jobs to verify data integrity
- **Resetting buckets** — Clear all content from buckets (destructive operation)
- **Reconciliation reports** — Detect configuration drift between local and remote bucket state
- **Transferring data** — Copy data from any source bucket to a destination stack bucket

## Computing a checksum

Triggers S3 batch checksum jobs for the specified bucket and its replication pair.

### CLI command

```bash
make run-compute-checksums b=<bucket-name> p=<profile>
```

### Parameters

- `b=` — Target a standard or public stack bucket to generate checksums for (required). The CLI will automatically match this with its replication bucket. Cannot specify a replication bucket directly.
- `p=` — AWS profile to use (required)

### Example

```bash
make run-compute-checksums b=digipres-dev1-private p=default
```

### Output

For each bucket pair processed, job receipts are uploaded to:
- `metadata/latest/checksums/receipts/{source_job_id}.json`
- `metadata/latest/checksums/receipts/{repl_job_id}.json`
- `metadata/latest/checksums/receipts/{source_bucket_name}.json`
- `metadata/{date}/checksums/receipts/{source_bucket_name}.json`

The receipt contains job metadata and is available for discovery by job ID (used internally) or bucket name (used by checksum report process).

## Resetting all content

> [!IMPORTANT]
> This is destructive and should be done very carefully.

### CLI commands

**Reset buckets (keep resources):**
```bash
make reset s=<stack-name> p=<profile>
```

Empties all bucket contents while keeping the stack infrastructure intact.

### Parameters

- `s=` — Stack name to reset (required)
- `p=` — AWS profile to use (required)

### Example

```bash
# Empty buckets only
make reset s=digipres-dev1 p=default
```

## Reconciliation report

TODO

## Transfer data to stack bucket

TODO
