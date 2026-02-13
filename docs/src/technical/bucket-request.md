# bucket-request

Lambda triggered by: s3 event (user uploaded file)
Dependencies: None

## Overview

This function is used to create s3 buckets with [prefab configuration](#).
To do this a text file is uploaded to `${stack}-bucket-request` then
downloaded for processing (either locally or in Lambda).

## CLI testing

```bash
make run-bucket-request f=files/buckets.txt s=digipress-dev1 p=default
```

- `f=` is the path to a local file that contains [bucket names](#).

## Remote testing

```bash
make upload b=digipress-dev1-bucket-request f=files/buckets.txt p=default
```

- `f=` is the path to a local file that contains [bucket names](#).

## Output

Give the example file `files/buckets.txt` four buckets should be created
(assuming they do not already exist).

- `digipres-dev1-private` (private s3 bucket)
- `digipres-dev1-private-repl` (private s3 bucket replication destination)
- `digipres-dev1-public` (public s3 bucket)
- `digipres-dev1-public-repl` (public s3 bucket replication destination)

## QA testing

Aside from the happy path here are variations to try:

- File too large
- File invalid (rename some other file `buckets.txt` i.e a jpg)
- Bucket names are too long or has invalid characters
- Too many bucket names (5 max, additionals are discarded)
- Bucket names are duplicates, the buckets already exist
