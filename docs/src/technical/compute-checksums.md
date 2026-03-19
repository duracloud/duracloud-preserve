# compute-checksums

- Lambda triggered by: scheduled eventbridge event
- Dependencies: None

## Overview

This function triggers S3 batch [compute checksum](#) jobs. It will do this in standard/public + replication bucket pairs.

## CLI testing

```bash
make run-compute-checksums b=digipres-dev1-private p=default
```

- `b=` a (standard or public) stack bucket to generate checksums for

The CLI only supports targeting a single bucket at a time. The bucket will be matched with its replication bucket. You cannot specify a replication bucket directly.

## Remote testing

```bash
make trigger f=compute-checksums s=digipres-dev1 p=default
```

This will trigger jobs for `ALL` stack buckets.

## Output

```json
{
    "StatusCode": 200,
    "ExecutedVersion": "$LATEST"
}
```

For each bucket pair processed a single job "receipt" is uploaded to these locations:

- `metadata/latest/checksums/receipts/{source_job_id}.json`
- `metadata/latest/checksums/receipts/{repl_job_id}.json`
- `metadata/latest/checksums/receipts/{source_bucket_name}.json`
- `metadata/{date}/checksums/receipts/{source_bucket_name}.json`

The receipt is the same content uploaded multiple times for discovery (job ids are used by the Lambda checksum report process, bucket names for the CLI checksum report and for easier access in general).

## QA testing

Confirm:

- Jobs are created without errors
- Jobs are completed successfully
- All receipt files are generated and avaiable at the expected paths
