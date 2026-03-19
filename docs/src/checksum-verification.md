# Checksum Verification

The process for checksum verification works like this:

## Source file upload

AWS S3 provides integrity guarantees on file upload. Using integrity checking mechanisms S3 validates the received data and rejects uploads where the computed checksum does not match. A successful upload response from S3 confirms that the stored object matches exactly what was transmitted. We presume AWS is correct in claiming this, and our system's integrity position begins at the point of successful upload.

- [Checking object integrity in Amazon S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity.html)
- [Checking object integrity for data uploads in Amazon S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity-upload.html)

The AWS cli can be used to view an object's checksum (`ChecksumCRC64NVME` by default):

```bash
aws s3api head-object --bucket ${bucket} --key ${key} --checksum-mode ENABLED
```

```json
{
    "AcceptRanges": "bytes",
    "LastModified": "2026-01-24T00:22:19+00:00",
    "ContentLength": 15310515,
    "ChecksumCRC64NVME": "V+va1ramtYo=",
    "ChecksumType": "FULL_OBJECT",
    "ETag": "\"822f9ffde463633f9a56df6d90b1dbb6\"",
    "VersionId": "HnU.prnfFqU2oJKqjIibty9_cet6zTDH",
    "ContentType": "application/pdf",
    "ServerSideEncryption": "AES256",
    "Metadata": {},
    "StorageClass": "GLACIER_IR",
    "ReplicationStatus": "COMPLETED"
}
```

## Replication

AWS S3 replication creates a copy of uploaded files within 15 minutes of upload with the same guarantees around file upload integrity (i.e. the replicated object is an exact copy of the source file).

- [Meeting compliance requirements with S3 Replication Time Control](https://docs.aws.amazon.com/AmazonS3/latest/userguide/replication-time-control.html)
- [Replicating objects within and across Regions](https://docs.aws.amazon.com/AmazonS3/latest/userguide/replication.html)

The AWS cli view of a replicated object's checksum will match the source object:

```json
{
    "AcceptRanges": "bytes",
    "LastModified": "2026-01-24T00:22:19+00:00",
    "ContentLength": 15310515,
    "ChecksumCRC64NVME": "V+va1ramtYo=",
    "ChecksumType": "FULL_OBJECT",
    "ETag": "\"822f9ffde463633f9a56df6d90b1dbb6\"",
    "VersionId": "HnU.prnfFqU2oJKqjIibty9_cet6zTDH",
    "ContentType": "application/pdf",
    "ServerSideEncryption": "AES256",
    "Metadata": {},
    "StorageClass": "GLACIER",
    "ReplicationStatus": "REPLICA"
}
```

Most significantly the `ChecksumCRC64NVME` and `VersionId` values match.

## Durability

Given AWS durability guarantees we believe with a high degree of confidence that uploaded and replicated objects are correct and consistent at the point of replication (i.e. integrity is preserved and they have the same checksum).

- [Durability in S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/DataDurability.html)

## Verification Process

We use S3 batch operations to generate checksum reports of all objects in source and replication buckets.

If object version and checksum match then we consider the verification to be successful.

If they do not match then one or the other file is corrupted. In this case a previously generated checksum report can be used to identify which file is corrupt.

If a prior report containing the objects in question is not available then request the object's metadata to get the stored checksum function, value and version to compare them. For more thorough inspection download the objects and compute the checksums locally using the same checksum function as S3 (`CRC-64/NVME` by default) to verify the state of the objects:

```bash
# get the originally computed checksum
aws s3api head-object --bucket ${bucket} --key ${key} --checksum-mode ENABLED
# download the file
aws a3 cp s3://${bucket}/${key} .
# checksum the file locally using the duracloud cli
duracloud checksum --file ${key}
```

Re-upload the checksum validated file to the source bucket to repair the original or replicated object.

- [Compute checksums](https://docs.aws.amazon.com/AmazonS3/latest/userguide/batch-ops-compute-checksums.html)
- [Examples: S3 Batch Operations completion reports](https://docs.aws.amazon.com/AmazonS3/latest/userguide/batch-ops-examples-reports.html)
- [Efficiently verify Amazon S3 data at scale with compute checksum operation](https://aws.amazon.com/blogs/storage/efficiently-verify-amazon-s3-data-at-scale-with-compute-checksum-operation/)

**For hosted clients Lyrasis will handle checksum verification and file restore if errors are found.**

## Successful verification

Successful verification confirms that the source and replica objects are identical to each other. Given S3's upload integrity guarantees and documented durability (99.999999999%), this means objects are also identical to what was originally uploaded to a very high degree of confidence. However, for the strongest guarantee of the latter independent verification using locally managed checksums is required (see Stricter Compliance Requirements below for more information and best practices).

- [Data protection in Amazon S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/DataDurability.html)
- [Amazon S3 Storage Classes](https://aws.amazon.com/s3/storage-classes/)

## Report Retention

Checksum reports are stored in S3 for the duration of the stack retention policy. Users can download these reports at any time. For users requiring independent verification or stricter compliance, we recommend downloading and storing reports locally or in a separate system.

## Summary

Checksum verification serves to detect silent data corruption (bit rot) that may occur over time, even in highly durable storage systems. By comparing checksums across independent copies on a regular schedule, we can identify and remediate corruption before it affects both copies.

We regard this strategy to be sufficient for the vast majority of standard use cases. We believe that objects in S3 are correct the vast majority of the time and if there is any corruption that is not automatically addressed by the S3 infrastructure then corruption wouldn't happen to both independent copies of a file in the exact same way (thereby creating a false impression that the checksums are verified when in reality both are corrupted).

## Stricter Compliance Requirements

If a greater guarantee of file integrity is required than has been described then a best practice is to create and manage a local checksum inventory (filename + checksum at least) before uploading files. After retrieving a file perform a local integrity check to confirm the retrieved file is exactly as expected. For the strictest compliance standards this is necessary in any case because it's the only way to not be wholly dependent on the claims of a third party (and of course some other 3rd party service or audit mechanism can be used so long as it's independent of one, single primary source).

We recommend using a tool like [QuickHash](https://www.quickhash-gui.org/) to generate checksums for all files before they are uploaded to S3. You must then keep the checksum files safe to reference in the future if a local checksum verification is required. You can store these files in S3.

For guidance on digital preservation standards and assessment frameworks, see:

- [NDSA Levels of Digital Preservation](https://ndsa.org/publications/levels-of-digital-preservation/) - A tiered framework for assessing digital preservation practices
- [Digital Preservation Coalition - Audit and Certification](https://www.dpconline.org/handbook/institutional-strategies/audit-and-certification) - Overview of audit standards and certification options

Also, for more highly regulated use cases, it's important to fully consider that DuraCloud Preserve is entirely dependent upon the Amazon AWS S3 service, their regional infrastructure and their policies.
