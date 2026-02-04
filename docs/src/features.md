# Features

This project is built on Amazon S3, a robust and distributed file storage service. You can think of the project as being in large part an extension of S3 that handles:

* Configuring more "complex" aspects of S3 to support storage and preservation goals.
* Providing additional value-added features via a set of scheduled tasks.

This is a brief overview of the functionality that is explained more throughly in the [user guide](user/index.html) and [technical documentation](technical/index.html).

## Configuration

**Versioning**

Bucket versioning is enabled. This supports file restore for up to N days post update.

**Lifecycle transitions**

Files are uploaded to the standard storage tier and transition to the Glacier Instant Retrieval tier after N days.

**Inventory**

A file manifest is generated for each user-created bucket.

**Replication**

Files for all buckets are replicated to GLACIER Deep Archive.

**Audit trail**

A request log is generated for each user-created bucket.

**Access controls**

Users can be standard or power users by assignment to a stack created IAM group.

* Standard users can list and upload files but cannot download or delete them.
* Power users can do all of the above.

Only account administrators can access replicated buckets and objects.

**Public access**

Buckets can be created as publicly accessible. Files will then be available using a standard, unauthenticated URL. Files will be stored in the standard storage tier and not transitioned to Glacier; however, replication will still occur, and the backup copies will be stored in Glacier.

**Choice of region**

Files can be stored in any AWS region supported by the infrastructure.

## Tasks

**Checksum reports**

Checksum reports are generated N times a year. This involves determining whether any files need to be replaced.

**Manifest reports**

A standard, easy to access conslidated csv file is generated per bucket.

**Storage reports**

A storage report document is generated to show usage stats.
