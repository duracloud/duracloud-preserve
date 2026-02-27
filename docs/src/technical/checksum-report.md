# checksum-report

- Lambda triggered by: cloudtrail eventbridge event (job status `complete` or `failed`)
- Dependencies: compute-checksums

## Overview

This function processes batch compute checksum reports (AWS batch job output) into a single checksum report csv per bucket and generates and uploads checksum verification stats (total mismatches etc.).

In production, this function is triggered by eventbridge events for batch job completion or failure. This happens asynchronously; each bucket (pair) job happens independently and will complete separately.

## CLI testing

```bash
make run-checksum-report b=digipres-dev1-private p=default
```

- `b=` a (standard or public) stack bucket to generate checksum report for

A compute checksums job must already have been run and completed for the bucket pair (source and replication) for the CLI process to have something to work with.

## Remote testing

Testing begins the same way as for `compute-checksums`:

```bash
make trigger-compute-checksums s=digipres-dev1 p=default
```

A compute checksum job, when completed, will automatically trigger the checksum report generation. This will happen once for each job (bucket), however, report generation cannot complete until both source and replication bucket (pair) jobs have completed, so the first time it is invoked it will exit.

**Note, completion can take a long time for replication buckets whose objects are in an archival storage tier (days), so for testing, it's recommended to review buckets with only recently created objects that haven't transitioned into the archival tier yet.**

You can track job status like this:

```bash
export AWS_PROFILE=default
export RECEIPT=digipres-dev1-private.json

aws s3 cp \
    s3://digipres-dev1-managed/metadata/latest/checksums/${RECEIPT} .

aws s3control describe-job \
    --account-id $(aws sts get-caller-identity --query 'Account' --output text) \
    --job-id $(cat ${RECEIPT} | jq -r .repl_job_id) |
    jq '{JobId: .Job.JobId, Status: .Job.Status}'
```

If the status is "Active", then it's still running.

## Output

- report csv
- verification stats

## QA testing

Confirm:

- Files are uploaded
- Appropriate logging for first bucket event (exit only)
- Appropriate logging for second bucket event (continuation)
