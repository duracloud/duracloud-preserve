# CLI

`dcp` is the command-line interface for DuraCloud Preserve. It provides access to all core functions — the same operations run by the deployed Lambda functions — as well as additional commands for local data validation, maintenance, and bucket management that are not available through Lambda.

## Usage

```bash
dcp <COMMAND> [OPTIONS]
```

## Commands

| Command | Description |
|---|---|
| `bucket-reconciliation` | Check bucket configuration and report drift |
| `bucket-request` | Process bucket creation requests |
| `checksum` | Compute a checksum for a local file |
| `checksum-inventory` | Build checksum inventory from S3 inventory data |
| `checksum-report` | Generate checksum report and statistics |
| `compute-checksums` | Run S3 batch operations compute checksums |
| `inventory-report` | Generate inventory report and statistics |
| `reset` | Reset stack (empty buckets, requires confirmation) |
| `storage-report` | Generate storage report |
| `sync-users` | Sync IAM users to SFTPGo |
| `transfer` | Transfer files from source to stack destination bucket |

Run `dcp help <COMMAND>` for detailed usage of any command.

## CLI-only

Beyond core functionality, the CLI provides features for data validation, maintenance, and bucket management. These commands are only available through the command-line interface:

- **Computing a checksum** — Compute a checksum for a local file to verify data integrity
- **Resetting buckets** — Clear all content from buckets (destructive operation)
- **Reconciliation reports** — Detect configuration drift between local and remote bucket state
- **Transferring data** — Copy data from any source bucket to a destination stack bucket

## Computing a checksum

Compute the checksum for a local file to verify data integrity.

### CLI command

```bash
dcp checksum [OPTIONS] <FILE>
```

### Parameters

- `<FILE>` — Path to the local file to checksum (required)

### Example

```bash
dcp checksum --file myfile.txt
```

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

Checks bucket configuration against expected state and reports any drift. Use this to verify that buckets are configured correctly and that no settings have diverged from the intended setup.

### CLI command

```bash
dcp bucket-reconciliation [OPTIONS]
```

### Parameters

- `--stack` — Stack name to reconcile (required)
- `--profile` — AWS profile to use (required)

### Example

```bash
dcp bucket-reconciliation --stack digipres-dev1 --profile default
```

### Output

A report is generated and uploaded to the `-managed` bucket detailing:
- Buckets with unexpected configuration
- Missing or extra settings compared to expected state
- A summary of drift found (or confirmation that no drift was detected)

## Transfer data to stack bucket

Copies files from any accessible source bucket to a destination bucket within the target stack. Useful for migrations and moving data between stacks or accounts.

### CLI command

```bash
dcp transfer [OPTIONS]
```

### Parameters

- `--source` — Source S3 bucket to transfer from (required)
- `--destination` — Destination stack bucket to transfer into (required)
- `--profile` — AWS profile to use (required)

### Example

```bash
dcp transfer --source old-stack-private --destination digipres-dev1-private --profile default
```

### Notes

- The destination bucket must be a valid stack bucket (i.e. prefixed with the stack name).
- Files are copied, not moved — the source bucket is not modified.
- Replication of transferred files to Glacier will occur automatically once files land in the destination bucket.
