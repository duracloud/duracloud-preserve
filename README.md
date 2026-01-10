# DuraCloud

Simplifies configuration, access control and preservation concerns using AWS S3.

## Summary

The goal is to make it easy for users to choose any off the shelf S3 client and interact with S3 gaining more advanced features by default. Advanced features are described in more detail below and in user documentation but in brief: versioning, inventory, replication, logging etc. is enabled as buckets are created without a user having to do anything in AWS. Periodically checksum verification is performed to ensure that file integrity is maintained between the primary and replicated (backup) files. This builds on the already impressive levels of durability provided by S3 by adding a further automated guarantee that files are what they are intended to be.

Links to additional reading (TODO).

Additional features include generating reports (reformatted inventory and storage) and user access control via preconstructed groups that are scoped to stacks. When DuraCloud is deployed every resource is created "within" a stack. A stack is simply a resource naming prefix and tag applied to all resources managed by the deployed components to exclusively associate them. This makes it possible to have multiple stacks within a single account and makes it so different users can belong to one or more stacks.

## Overview

AWS resources used:

- events (event bridge, s3)
- lambda
- s3

### bucket-request

Triggered by: s3 event

Create new user buckets upon upload of file containing bucket names, and applies prefab configuration.

### process-inventory

Triggered by: s3 event

1. Processes parquet formatted inventory into a single human readable csv.
2. Generates storage usage stats including by (top level) prefix.

### generate-checksums

Triggered by: eventbridge event (1st of month)

Starts S3 batch jobs to generate checksum reports.

### checksum-verification

Triggered by: eventbridge event (2nd of month)

Compares checksum reports for source and replication destination buckets.

### storage-report

Triggered by: eventbridge event

Generates a consolidated storage report for all buckets.
