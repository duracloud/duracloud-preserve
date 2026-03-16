# checksum-report

> **Trigger:** CloudTrail EventBridge event (batch job status: `complete` or `failed`)  
> **Dependencies:** [`compute-checksums`](compute-checksums.md)

## Overview

`checksum-report` processes AWS Batch compute checksum job output into a single checksum report CSV per bucket, and generates checksum verification stats (e.g. total mismatches).

In production, this function is triggered asynchronously by EventBridge on batch job completion or failure. Each bucket pair job runs independently and completes separately.

## Usage

### CLI (local testing)
> [!IMPORTANT] 
> A `compute-checksums` job must already have run and completed for the target bucket pair (source + replication) before using this command.
```bash
make run-checksum-report b=digipres-dev1-private p=default
```

| Flag | Description                                                         |
| ---- | ------------------------------------------------------------------- |
| `b=` | A standard or public stack bucket to generate a checksum report for |
| `p=` | AWS profile                                                         |

### Remote testing

Remote testing starts the same way as [`compute-checksums`](#):
```bash
make trigger-compute-checksums s=digipres-dev1 p=default
```

When a compute checksum job completes, it automatically triggers checksum report generation — once per bucket job. However, report generation requires **both** the source and replication bucket jobs to be complete. If the first job finishes before the second, the function will exit early and wait.

> [!Note]
> Replication buckets with objects in an archival storage tier can take days to complete. For testing, use buckets that contain only recently created objects that haven't yet transitioned to archival storage.

#### Tracking job status
```bash
export AWS_PROFILE=default
export RECEIPT=digipres-dev1-private.json

# Download the job receipt
aws s3 cp \
    s3://digipres-dev1-managed/metadata/latest/checksums/${RECEIPT} .

# Check job status
aws s3control describe-job \
    --account-id $(aws sts get-caller-identity --query 'Account' --output text) \
    --job-id $(cat ${RECEIPT} | jq -r .repl_job_id) \
    | jq '{JobId: .Job.JobId, Status: .Job.Status}'
```

A status of `"Active"` means the job is still running.

## Expected output

| Output             | Description                                |
| ------------------ | ------------------------------------------ |
| Report CSV         | Per-bucket checksum report                 |
| Verification stats | Summary metrics including total mismatches |

## QA testing

Confirm:

- Files are uploaded
- Appropriate logging for first bucket event (exit only)
- Appropriate logging for second bucket event (continuation)
