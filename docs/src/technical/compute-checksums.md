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
mise run compute-checksums --bucket digipres-dev1-private --profile default
```

**Parameters:**
- `--bucket` — Standard or public stack bucket to checksum (required)
- `--profile` — AWS profile (required)

**Constraints:**
- Only supports single bucket at a time
- Automatically paired with replication bucket
- Cannot directly specify a replication bucket

### Remote trigger

Compute checksums for all stack buckets in a given stack:

```bash
mise run trigger --function compute-checksums --stack digipres-dev1 --profile default
```

**Parameters:**
- `--function` — Function name (compute-checksums)
- `--stack` — Stack name (required)
- `--profile` — AWS profile (required)

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
