# compute-checksums

**Type:** Lambda function  
**Trigger:** Scheduled EventBridge event  
**Dependencies:** None

## Overview

This Lambda function triggers S3 batch checksum jobs to verify data integrity across your buckets. It processes standard/public + replication bucket pairs together, ensuring both the source and replicated data are checksummed.

## Invocation methods

### Scheduled execution (production)

The Lambda is automatically triggered by a scheduled EventBridge event at regular intervals.

### CLI testing

Compute checksums for a single bucket and its replication pair:

```bash
make run-compute-checksums b=digipres-dev1-private p=default
```

**Parameters:**
- `b=` — Standard or public stack bucket to checksum (required)
- `p=` — AWS profile (required)

**Constraints:**
- Only supports single bucket at a time
- Automatically paired with replication bucket
- Cannot directly specify a replication bucket

### Remote trigger

Compute checksums for all stack buckets in a given stack:

```bash
make trigger f=compute-checksums s=digipres-dev1 p=default
```

**Parameters:**
- `f=` — Function name (compute-checksums)
- `s=` — Stack name (required)
- `p=` — AWS profile (required)

**Behavior:** Triggers jobs for ALL stack buckets in the specified stack.

## Output

### Function response

```json
{
    "StatusCode": 200,
    "ExecutedVersion": "$LATEST"
}
```

### Receipt files

For each bucket pair processed, a job receipt is uploaded to:

- `metadata/0000-00-00-LATEST/checksums/receipts/{source_job_id}.json`
- `metadata/0000-00-00-LATEST/checksums/receipts/{repl_job_id}.json`
- `metadata/0000-00-00-LATEST/checksums/receipts/{source_bucket_name}.json`
- `metadata/{date}/checksums/receipts/{source_bucket_name}.json`

**Purpose:** The receipt is uploaded multiple times for different discovery paths:
- Job IDs — used by the Lambda checksum report process for internal tracking
- Bucket names — used by the CLI checksum report and for easier manual access

## QA testing
Confirm:
- Jobs are created without errors
- Jobs are completed successfully
- All receipt files are generated and avaiable at the expected paths
