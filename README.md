# DuraCloud

Simplified configuration, access control and digital preservation for AWS S3.

## Summary

The goal of DuraCloud is to make it easy for users to choose any off the shelf S3 client and interact with S3 gaining more advanced features by default. Advanced features are described in more detail below and in user documentation but in brief: versioning, inventory, replication, logging etc. is enabled as buckets are created without a user having to do anything in AWS. Periodically checksum verification is performed to ensure that file integrity is maintained between the primary and replicated (backup) files. This builds on the already impressive levels of durability provided by S3 in adding a further automated guarantee that files are what they are intended to be.

Links to additional reading (TODO).

Additional features include generating reports (reformatted inventory and storage) and user access control via preconstructed groups that are scoped to stacks. When DuraCloud is deployed every resource is created "within" a stack. A stack is simply a resource naming prefix and tag applied to all resources managed by the deployed components to exclusively associate them. This makes it possible to have multiple stacks within a single account and makes it so different users can belong to one or more stacks.

## Overview

AWS resources used:

- events (event bridge, s3)
- lambda
- s3

### bucket-request

Triggered by: s3 event (user uploaded file)
Dependencies: None

Create new user buckets upon upload of file containing bucket names, and applies prefab configuration.

### inventory-report

Triggered by: s3 event (manifest.checksum)
Dependencies: None

1. Processes parquet formatted inventory into a single human readable csv.
2. Generates storage usage stats including by (top level) prefix.

### compute-checksums

Triggered by: scheduled eventbridge event (schedule: TBD)
Dependencies: None

Trigger S3 batch compute checksum jobs.

### checksum-report

Triggered by: cloudtrail eventbridge event (job status complete or failed)
Dependencies: compute-checksums

1. Processes batch compute checksum reports into a single checksum report csv.
2. Generates checksum verification stats (total mismatches etc.).

### storage-report

Triggered by: scheduled eventbridge event (schedule: daily)
Dependencies: inventory-report

Generates a consolidated storage report for all buckets.
