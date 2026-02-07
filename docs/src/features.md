# Features

This project is built on Amazon S3, a robust and distributed file storage service. You can think of the project as being in large part an extension of S3 that handles:

* Configuring more "complex" aspects of S3 to support long-term storage and preservation goals.
* Providing additional value-added features via a set of scheduled tasks.

This is a brief overview of the functionality that is explained more throughly in the [user guide](user/index.html) and [technical documentation](technical/index.html):

---

## Access controls

Users can be standard or power users by assignment to a stack created IAM group.

* Standard users can list and upload files but cannot download or delete them.
* Power users can do all of the above.

Only AWS account administrators can access replicated buckets and objects.

## Audit trail

Request logs are generated for each user-created bucket. This is raw AWS provided data that can be processed using tools like [DuckDB](#).

## Checksum reports

Checksum reports are generated N times a year. This involves determining whether any files need to be replaced.

## Choice of region

Files can be stored in any AWS region supported by the infrastructure.

## Inventory

A file manifest is generated for each user-created bucket. The raw AWS inventory data is available in Parquet format but a consolidated, user friendly csv file is also made available that includes the S3 url for each file.

## Lifecycle transitions

Files are uploaded to the standard storage tier and transition to the Glacier Deep Archive tier after N days where N can be configured for each stack deployment.

## Manifest reports

A standard, easy to access conslidated csv file is generated per bucket.

## Public access

Buckets can be created as publicly accessible. Files will then be available using a standard, unauthenticated URL. Files will be stored in the intelligent storage tier and not transitioned to Glacier; however replication will still occur and the backup copies will be stored in Glacier.

## Replication

Files for all buckets are replicated to Glacier Deep Archive. These files are included in the checksum verification process to determine file integrity. We have dedicated documentation for how this works.

## Storage reports

A storage report document is generated to show usage stats.

## Versioning

Bucket versioning is enabled. This supports file restore for up to N days post update where N can be configured for each stack deployment.
