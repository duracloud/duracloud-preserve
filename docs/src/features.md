# Features

This is a brief overview of the functionality that is explained more thoroughly in the [user guide](user/README.md) and [technical documentation](technical/README.md):

## Access controls

Users can be standard or power users by assignment to a stack created IAM group.

* Standard users can list and upload files but cannot download or delete them.
* Restricted users can list and upload files within designated buckets but cannot download or delete them.
* Power users can list, upload, download, and delete files.

Only AWS account administrators can access replicated buckets and objects.

## Audit trail

Request logs are generated for each user-created bucket. This is raw AWS provided data that can be processed using tools like [DuckDB](https://duckdb.org/).

## Checksum reports

Checksum reports are generated on a configurable schedule, comparing checksums across source and replication buckets to detect corruption. Files found to be corrupt can be restored from the verified copy. See the [checksum verification](./checksum-verification.md) documentation for more details.

## Choice of region

Files can be stored in any AWS region supported by the infrastructure.

## CLI available

A command-line interface (`dcp`) is available for advanced users. It provides access to all core functions and additional maintenance commands for tasks such as checksumming local files, reconciling bucket configuration, and transferring data between buckets.

## Hosting and support

If creating an AWS account and deploying resources to it is not possible then Lyrasis provides a [hosting and technical support](./lyrasis.md) option to handle the infrastructure for you.

## Lifecycle transitions

Files are uploaded to the standard storage tier and transition to a selected storage class after a configurable interval, which can be specified for each stack deployment. Old versions of files and aborted multipart uploads are automatically deleted after a configurable period.

## Manifest reports

A file manifest is generated for each user-created bucket. The raw inventory data is available in Parquet format and a consolidated, human-readable CSV file is generated listing all files with metadata including S3 URL, size, storage class, and last modified date.

## Public access via CDN (Content Delivery Network)

A [CloudFront](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/Introduction.html) distribution and bucket is created that can be used to make files publicly available. Simply upload files to it and share the public url using a specified domain.

Other buckets can be created as publicly accessible by naming them with a `-public` suffix. Files uploaded to such buckets will be available using a standard, unauthenticated S3 URL.

Files will be stored in the intelligent storage tier and not transitioned to Glacier; however replication will still occur and the backup copies will be stored in Glacier.

## Reconciliation reports

The reconciliation report is used to detect drift in bucket configuration, providing reassurance that buckets are configured correctly and working as expected.

## Replication

Files for all buckets are replicated to Glacier Deep Archive. These files are included in the checksum verification process to determine file integrity. We have [dedicated documentation](./checksum-verification.md) for how this works.

## Storage reports

An HTML storage report is generated showing usage statistics across all buckets in the stack, including total file counts and storage consumed by bucket and top-level prefix. It also includes the year-to-date total of data transfer out from S3 to the internet (requires Cost Explorer to be enabled in AWS, and an active `Stack` cost allocation tag).

## Versioning

Bucket versioning is enabled. This supports file restore for up to a configurable number of days post update which can be specified for each stack deployment.

## Web UI integration with SFTPGo

There is support within the application and deployment tooling for SFTPGo integration, which provides a web based interface for S3. Users can be created that are pre-configured with appropriate access (per the access controls that have been assigned to them) and the SFTPGo user account is kept in sync as buckets are created, or via the `dcp` cli.

---

## General integrations

### Web applications that support use of Amazon S3 for storage

Any application or framework that can be configured to use Amazon S3 for storage can work with DuraCloud Preserve. By simply using a bucket created as part of a DuraCloud Preserve stack files will be stored with the additional benefits outlined in this documentation, including versioning, replication and checksum verification.

Some specific examples:

* Any [Rails](https://rubyonrails.org/) web applications using [ActiveStorage](https://guides.rubyonrails.org/active_storage_overview.html#s3-service-amazon-s3-and-s3-compatible-apis).
* [Archivematica Storage Service](https://www.archivematica.org/en/docs/storage-service-0.24/administrators/#s3-amazon)
* [CollectionSpace file storage](https://collectionspace.atlassian.net/wiki/spaces/cstd/pages/3576725711/Configuring+Amazon+S3+plugin).
* [DSpace Storage Layer](https://wiki.lyrasis.org/display/DSDOC9x/Storage+Layer#StorageLayer-ConfiguringAmazonS3Storage).

## Lyrasis service integrations

### ArchivesSpace

ArchivesSpace itself does not manage digital content and provides no way to upload files. The public urls provided by the Duracloud Preserve CloudFront enabled bucket can be used to host files that are referenced in Digital Objects using the File URI field to make them openly accessible on the internet.

### CollectionSpace

Refer to the [roadmap](#) for any upcoming work.

### DSpace

The [Replication Task Suite](https://wiki.lyrasis.org/display/DSPACE/ReplicationTaskSuite) is a plugin for DSpace that adds preservation capabilities that can be accessed using the DSpace user interface. It creates archival information packages used to backup DSpace items in a self contained way that are periodically transferred to external storage, including Amazon S3. Doing the latter with a DuraCloud Preserve created bucket works equivalently to using S3 for the DSpace Storage Layer (assetstore), and if both are configured this way it enables a dual layer of protection for files (as both the assetstore and archival packages would benefit from versioning, replication and checksum verification etc.).

## Other integrations

### Archive-It

Create an inventory and a backup of [WARC](https://en.wikipedia.org/wiki/WARC_(file_format)) files retrieved from the [Internet Archive](https://archive.org/) - [Archive-It](https://archive-it.org/) service.
