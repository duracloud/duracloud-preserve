# Features

This project is built on Amazon S3, a robust and distributed file storage service. You can think of the project as being in large part an extension of S3 that handles:

* Configuring the more "complex" aspects of S3 to support long-term storage and preservation goals without a regular user needing to understand how to glue things together.
* Preconfiguring [CloudFront](#) access for making files publicly available.
* Providing additional value-added features via a set of scheduled tasks.

This is a brief overview of the functionality that is explained more thoroughly in the [user guide](user/README.md) and [technical documentation](technical/README.md):

## Access controls

Users can be standard or power users by assignment to a stack created IAM group.

* Standard users can list and upload files but cannot download or delete them.
* Power users can do all of the above.

Only AWS account administrators can access replicated buckets and objects.

## Audit trail

Request logs are generated for each user-created bucket. This is raw AWS provided data that can be processed using tools like [DuckDB](https://duckdb.org/).

## Checksum reports

Checksum reports are generated on a configurable schedule, comparing checksums across source and replica buckets to detect corruption. Files found to be corrupt can be restored from the replicated copy.

## Choice of region

Files can be stored in any AWS region supported by the infrastructure.

## CLI available

A command-line interface (`dcp`) is available for advanced users. It provides access to all core functions and additional maintenance commands for tasks such as checksumming local files, reconciling bucket configuration, and transferring data between buckets.

## Hosting and support

If creating an AWS account and deploying resources to it is not possible then Lyrasis provides a [hosting and technical support](./lyrasis.md) option to handle the infrastructure for you.

## Inventory

A file manifest is generated for each user-created bucket. The raw AWS inventory data is available in Parquet format and a consolidated, user friendly csv file is also made available that includes the S3 url for each file.

## Lifecycle transitions

Files are uploaded to the standard storage tier and transition to the Glacier Deep Archive tier after a configurable number of days which can be specified for each stack deployment.

## Manifest reports

A consolidated, human-readable CSV file is generated per bucket, listing all files with metadata including S3 URL, size, storage class, and last modified date.

## Public access

A [CloudFront](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/Introduction.html) distribution and bucket is created that can be used to make files publicly available. Simply upload files to it and share the public url using a specified domain.

Other buckets can be created as publicly accessible by naming them with a `-public` suffix. Files uploaded to such buckets will be available using a standard, unauthenticated S3 URL.

Files will be stored in the intelligent storage tier and not transitioned to Glacier; however replication will still occur and the backup copies will be stored in Glacier.

## Reconciliation reports

The reconciliation report is used to detect drift in bucket configuration, providing reassurance that buckets are configured correctly and working as expected.

## Replication

Files for all buckets are replicated to Glacier Deep Archive. These files are included in the checksum verification process to determine file integrity. We have [dedicated documentation](./checksum-verification.md) for how this works.

## Storage reports

An HTML storage report is generated showing usage statistics across all buckets in the stack, including total file counts and storage consumed by bucket and top-level prefix.

## Versioning

Bucket versioning is enabled. This supports file restore for up to a configurable number of days post update which can be specified for each stack deployment.
