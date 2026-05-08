# checksum-report

> **Trigger:** CloudTrail EventBridge event (batch job status: `complete` or `failed`)  
> **Dependencies:** [`compute-checksums`](compute-checksums.md)

## Overview

`checksum-report` processes AWS Batch compute checksum job output into a single checksum report CSV per bucket, and generates checksum verification stats (e.g. total mismatches).

In production, this function is triggered asynchronously by EventBridge when a batch job reaches `complete` or `failed` status. Each bucket pair (source + replication) runs as independent jobs. Report generation requires **both** jobs to be complete — if the first job finishes before the second, the function exits early and waits for the second event before continuing.

## Usage

### CLI (local testing)

> [!IMPORTANT]
> [`compute-checksums`](compute-checksums.md) must have already run and completed for the target bucket pair (source + replication) before running this command.

```bash
make run-checksum-report b=digipres-dev1-private p=default
```

| Flag | Description                                                         |
| ---- | ------------------------------------------------------------------- |
| `b=` | A standard or public stack bucket to generate a checksum report for |
| `p=` | AWS profile                                                         |

### Remote testing

Remote testing starts the same way as [`compute-checksums`](compute-checksums.md):

```bash
make trigger f=compute-checksums s=digipres-dev1 p=default
```

When a compute checksum job completes, it automatically triggers checksum report generation — once per bucket job.

> [!Note]
> Replication buckets with objects in glacier storage tier can take days to complete. For testing, use buckets that contain only recently created objects that haven't yet transitioned to glacier storage.

#### Tracking job status

```bash
make job-status-by-receipt b=digipres-dev1-private p=default
```

A status of `"Active"` means the job is still running.

## Expected output

On success, the CLI prints a verification summary and uploads a report CSV to the managed bucket:

```
Checksum report complete:
        Total objects:      6
        Matches:            6
        Mismatches:         0
        Missing replica:    0
        Missing source:     0
        Failed source:      0
        Failed replication: 0
```

| Field                | Description                                              |
| -------------------- | -------------------------------------------------------- |
| `Total objects`      | Number of source objects evaluated                       |
| `Matches`            | Objects where source and replica checksums are identical |
| `Mismatches`         | Objects where checksums differ — indicates data integrity issue |
| `Missing replica`    | Objects present in source but absent from replication bucket |
| `Missing source`     | Objects present in replication but absent from source bucket |
| `Failed source`      | Objects where checksum computation failed on the source  |
| `Failed replication` | Objects where checksum computation failed on the replica |

A report CSV is also uploaded to the stack's managed bucket for long-term record keeping.

To verify the checksum report was written to S3:
```bash
aws s3 ls s3://digipres-dev1-managed/reports/$(date +%F)/checksums/
```

## QA testing

Confirm:

- Files are uploaded
- Appropriate logging for first bucket event (exit only)
- Appropriate logging for second bucket event (continuation)
