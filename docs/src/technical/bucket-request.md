# bucket-request

- **Lambda trigger:** S3 event (fires when a user uploads a file to the request bucket)
- **Dependencies:** None

## Overview

This Lambda function creates S3 buckets with [prefab configuration](#) based on a list of bucket names provided in a plain text file.

**Example buckets.txt**

```bash
manuscripts
newspapers
rare-books
```

The workflow is:

1. A text file containing bucket names is uploaded to the S3 bucket named `${stack}-request`
2. The Lambda function is triggered by the upload event
3. The file is downloaded and processed — either locally (for development/testing) or inside Lambda (for remote execution)
4. Buckets are created according to the prefab configuration if they don't already exist

## CLI Testing

Use `make run-bucket-request` to process a file locally without uploading to S3:

```bash
make run-bucket-request f=files/buckets-list.txt s=digipres-dev1 p=default
```

- `f=` — path to a local file containing [bucket names](#)
- `s=` — the stack name (used as a prefix for created buckets)
- `p=` — the AWS profile to use

You can also create a single bucket by name without a file, using the `cargo` CLI directly:

```bash
cargo run -p dcp -- bucket-request --stack=digipres-dev1 --name=rare-books
```

This is useful for one-off bucket creation or quick iteration without maintaining a file.

> [!IMPORTANT]
> Export your aws profile prior to using the `cargo` CLI.

## Remote Testing

Use `make upload` to upload a file to S3 and trigger the Lambda function as it would run in production:

```bash
make upload b=digipres-dev1-request d=buckets f=files/buckets.txt p=default
```

- `b=` — the name of the S3 request bucket (typically `${stack}-request`)
- `d=` — the S3 directory (path) to upload into (must be `bucket-request`)
- `f=` — path to the local file containing [bucket names](#)
- `p=` — the AWS profile to use

## Output

Given the example file `files/buckets.txt`, two buckets should be created (assuming they do not already exist):

- `digipres-dev1-private` — private S3 bucket
- `digipres-dev1-private-repl` — private S3 bucket used as the replication destination for the above

You can verify the buckets were created using:

```bash
make bucket a=list p=default
# Filter results by stack name using grep
make bucket a=list p=default | grep digipres-dev1
```

## QA testing

Aside from the happy path, here are variations to try:

- File too large
- File invalid (rename some other file `buckets.txt` i.e a jpg)
- Bucket names are too long or has invalid characters
- Too many bucket names (5 max, additionals are discarded)
- Bucket names are duplicates, the buckets already exist
- Errors should be uploaded to a file in the managed bucket `feedback` path
