# bucket-request

- Lambda triggered by: s3 event (user uploaded file)
- Dependencies: None

## Overview

This function creates Amazon S3 buckets with [prefab configuration](#).

A text file containing bucket names is uploaded to the `${stack}-bucket-request` bucket. When the file is uploaded, the S3 event triggers the Lambda function, which downloads the file and processes each bucket name.

## Usage

### CLI (local testing)

Run the function locally against a file of bucket names:
```bash
make run-bucket-request f=files/buckets.txt s=digipress-dev1 p=default
```

| Flag | Description                                       |
| ---- | ------------------------------------------------- |
| `f=` | Path to a local file containing [bucket names](#) |
| `s=` | Stack name                                        |
| `p=` | AWS profile                                       |

To create a single bucket instead of using a file:
```bash
cargo run -p duracloud -- bucket-request --stack=digipres-dev1 --name=rare-books
```

### Remote testing (uploads file to S3 to trigger Lambda)
```bash
make upload b=digipress-dev1-bucket-request f=files/buckets.txt p=default
```

| Flag | Description                                       |
| ---- | ------------------------------------------------- |
| `b=` | Target S3 bucket name                             |
| `f=` | Path to a local file containing [bucket names](#) |
| `p=` | AWS profile                                       |

## Expected output

Using the example file `files/buckets.txt`, two buckets should be created (if they don't already exist):

| Bucket                       | Type                                        |
| ---------------------------- | ------------------------------------------- |
| `digipres-dev1-private`      | Private S3 bucket                           |
| `digipres-dev1-private-repl` | Private S3 bucket (replication destination) |

Verify the buckets were created:
```bash
make bucket a=list p=default

# Filter results by stack name
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
