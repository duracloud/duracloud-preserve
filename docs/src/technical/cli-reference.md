# CLI Reference

The `dcp` command-line tool provides access to core operations for managing buckets, generating reports, and maintaining data integrity. This reference documents all available commands and their usage.

## Commands

| Command | Description |
|---|---|
| `bucket-reconciliation` | Check bucket configuration and report drift |
| `bucket-request` | Process bucket creation requests |
| `checksum` | Compute a checksum for a local file |
| `checksum-request` | Build checksum inventory from S3 inventory data |
| `checksum-report` | Generate checksum report and statistics |
| `compute-checksums` | Run S3 batch operations compute checksums |
| `inventory-report` | Generate inventory report and statistics |
| `reset` | Reset stack (empty buckets, requires confirmation) |
| `storage-report` | Generate storage report |
| `sync-users` | Sync IAM users to SFTPGo |
| `transfer` | Transfer files from source to stack destination bucket |

## Usage

```bash
dcp <COMMAND> [OPTIONS]
```

### Global options

- `-h, --help` — Print help message

## Commands

### Bucket operations

#### bucket-reconciliation

Check bucket configuration and report drift.

```bash
dcp bucket-reconciliation [OPTIONS]
```

Detects inconsistencies between local bucket configuration and remote state, useful for identifying configuration drift or missing objects.

---

#### bucket-request

Process bucket creation requests.

```bash
dcp bucket-request [OPTIONS]
```

Handle requests to create new buckets within the stack infrastructure.

---

#### reset

Reset stack (empty buckets, requires confirmation).

```bash
dcp reset [OPTIONS]
```

> [!CAUTION]
> This is a destructive operation. Removes all content from stack buckets. Requires confirmation before proceeding.

---

#### transfer

Transfer files from source to stack destination bucket.

```bash
dcp transfer [OPTIONS]
```

Copy data from a source bucket to a destination bucket within the stack. Useful for migrations and data reorganization.

---

### Checksum operations

#### checksum

Checksum a file.

```bash
dcp checksum [OPTIONS] <FILE>
```

Compute checksum for a local file to verify data integrity.

---

#### compute-checksums

Run S3 batch operations compute checksums.

```bash
dcp compute-checksums [OPTIONS]
```

Trigger S3 batch checksum jobs for buckets. For detailed usage, see [compute-checksums documentation](compute-checksums.md).

---

#### checksum-request

Build checksum inventory from S3 inventory data.

```bash
dcp checksum-request [OPTIONS]
```

Process S3 inventory data to create a checksum inventory for analysis and verification.

---

#### checksum-report

Generate checksum report and statistics.

```bash
dcp checksum-report [OPTIONS]
```

Create a report of checksum results and statistics across buckets. For detailed usage, see [checksum-report documentation](checksum-report.md).

---

### Reporting operations

#### inventory-report

Generate inventory report and statistics.

```bash
dcp inventory-report [OPTIONS]
```

Create an inventory report from S3 inventory data showing bucket contents and statistics. For detailed usage, see [inventory-report documentation](inventory-report.md).

---

#### storage-report

Generate storage report.

```bash
dcp storage-report [OPTIONS]
```

Generate a comprehensive storage report with visualizations showing storage usage across all buckets in the stack. For detailed usage, see [storage-report documentation](storage-report.md).

---

### User management

#### sync-users

Sync IAM users to SFTPGo.

```bash
dcp sync-users [OPTIONS]
```

Synchronize IAM users with SFTPGo for SFTP access management. For detailed usage, see [sync-users documentation](sync-users.md).

---

### Help

#### help

Print help message or help for a specific subcommand.

```bash
dcp help [COMMAND]
```

Display general help or help for a specific command.

## Common workflows

### Local testing with CLI

Most development and testing uses the CLI. See [development documentation](development.md) for local testing patterns.

### Testing with deployed Lambda

For testing with deployed Lambda functions, see the documentation for specific operations:

- [Compute checksums](compute-checksums.md)
- [Storage reports](storage-report.md)
- [Inventory reports](inventory-report.md)

## mise task helpers

The project provides [mise tasks](https://mise.jdx.dev/tasks/) that wrap CLI commands with common parameters:

```bash
# Example: Run compute-checksums via mise
mise run compute-checksums --bucket digipres-dev1-private --profile default

# Example: Trigger Lambda function
mise run trigger --function storage-report --stack digipres-dev1 --profile default

# Example: Run CLI command directly
dcp compute-checksums --bucket digipres-dev1-private
```

For all available tasks, run `mise tasks`.
