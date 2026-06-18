# Checksum Verification

DuraCloud Preserve stores and replicates files using Amazon S3. Checksum verification is the process by which the system confirms that stored files have not been silently corrupted over time. Even in highly durable storage systems, subtle errors (known as "bit rot") can alter file content without any obvious warning. By regularly comparing checksums across independent copies of each object, the system can detect and remediate corruption before it affects both copies.

## How It Works

### 1. Upload Integrity

AWS S3 provides integrity guarantees at the point of upload. Using built-in integrity checking mechanisms, S3 validates received data and rejects any upload where the computed checksum does not match. A successful upload response from S3 confirms that the stored object matches exactly what was transmitted.

The system's integrity guarantee begins at this point of successful upload.

The checksum and version of any stored object can be retrieved using the AWS CLI:

```bash
aws s3api head-object --bucket ${bucket} --key ${key} --checksum-mode ENABLED
```

Example response:

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

**Further reading:**

- [Checking object integrity in Amazon S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity.html)
- [Checking object integrity for data uploads in Amazon S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity-upload.html)

### 2. Replication

After a successful upload, AWS S3 replication creates a copy of the object in a second independent bucket, typically within 15 minutes. The same upload integrity guarantees apply to replication, ensuring the replicated object is an exact copy of the source.

The checksum and version ID of the replica will match the source object exactly:

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

Note that `ChecksumCRC64NVME` and `VersionId` are identical across both objects.

**Further reading:**

- [Meeting compliance requirements with S3 Replication Time Control](https://docs.aws.amazon.com/AmazonS3/latest/userguide/replication-time-control.html)
- [Replicating objects within and across Regions](https://docs.aws.amazon.com/AmazonS3/latest/userguide/replication.html)

### 3. Durability

AWS S3 is designed for 99.999999999% (eleven nines) durability. Given S3's upload integrity guarantees and its documented durability, uploaded and replicated objects can be considered correct and consistent at the point of replication with a very high degree of confidence.

**Further reading:** [Durability in Amazon S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/DataDurability.html)

### 4. Ongoing Verification

S3 Batch Operations are used to generate checksum reports across all objects in both the source and replication buckets. These reports are compared on a regular schedule.

| Result | Meaning |
|---|---|
| Version ID and checksum **match** | Verification successful — objects are identical |
| Version ID or checksum **do not match** | One object may be corrupted — investigation required |

## If a Mismatch Is Detected

If verification finds that checksums do not match, the following steps identify and repair the corruption.

**Step 1 — Check prior reports.** A previously generated checksum report may already contain the expected checksum values, making it straightforward to determine which copy — source or replica — is corrupt.

**Step 2 — Request object metadata.** If no prior report is available, retrieve the stored checksum, value, and version directly from each object's metadata and compare them:

```bash
aws s3api head-object --bucket ${bucket} --key ${key} --checksum-mode ENABLED
```

**Step 3 — Download and verify locally.** For a more thorough inspection, download the objects and compute checksums locally using an algorithm included in the object metadata (S3 uses `CRC-64/NVME` by default but other checksums may be present in addition to or instead of `crc64nvme` depending on how the object was uploaded):

```bash
# Retrieve the stored checksum
aws s3api head-object --bucket ${bucket} --key ${key} --checksum-mode ENABLED

# Download the file
aws s3 cp s3://${bucket}/${key} .

# Compute the checksum locally using the DuraCloud Preserve CLI
dcp checksum --file ${key}
```

**Step 4 — Restore.** Once the valid copy is confirmed, re-upload it to the source bucket to repair the corrupted object.

> [!IMPORTANT]
> **Hosted clients:** Lyrasis will handle checksum verification and file restoration on your behalf if errors are found.

Learn more about [Lyrasis Hosting](lyrasis.md)

**Further reading:**

- [Compute checksums](https://docs.aws.amazon.com/AmazonS3/latest/userguide/batch-ops-compute-checksums.html)
- [Examples: S3 Batch Operations completion reports](https://docs.aws.amazon.com/AmazonS3/latest/userguide/batch-ops-examples-reports.html)
- [Efficiently verify Amazon S3 data at scale with compute checksum operation](https://aws.amazon.com/blogs/storage/efficiently-verify-amazon-s3-data-at-scale-with-compute-checksum-operation/)

## What Successful Verification Confirms

Successful verification confirms that the source and replica objects are identical to each other. Given S3's upload integrity guarantees and its documented durability, this means objects are also identical to what was originally uploaded to a very high degree of confidence.

This strategy is considered sufficient for the vast majority of standard use cases. In the unlikely event that corruption is not automatically addressed by the S3 infrastructure, it is highly improbable that both independent copies would be corrupted in exactly the same way — which would be required to produce a false verification result.

For the strongest possible guarantee, independent verification using locally managed checksums is required. See [Stricter Compliance Requirements](#stricter-compliance-requirements) below.

**Further reading:**

- [Data protection in Amazon S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/DataDurability.html)
- [Amazon S3 Storage Classes](https://aws.amazon.com/s3/storage-classes/)

## Checksum Reports

Checksum reports are stored in S3 for the duration of the stack's retention policy and can be downloaded at any time.

For organizations requiring independent verification or stricter compliance, reports should be downloaded and stored locally or in a system separate from S3.

## Stricter Compliance Requirements

For organizations with higher assurance requirements — such as regulated industries or formal digital preservation programs — the approach described above may not be sufficient on its own, as it is ultimately dependent on the claims of a single third-party provider (Amazon AWS). An independent audit mechanism, separate from the primary storage provider, is required for the strictest compliance standards.

**Best practice for stricter compliance:**

1. **Generate checksums locally before uploading.** Use a tool such as [QuickHash](https://www.quickhash-gui.org/) to compute a checksum for each file before it is uploaded to S3.
2. **Maintain a local checksum inventory.** Keep a record of each filename and its corresponding checksum in a safe location. This inventory can be stored in S3, but must also exist independently.
3. **Verify on retrieval.** When downloading a file, recompute its checksum locally and compare it against the inventory record.

It is also important to note that DuraCloud Preserve is entirely dependent on the Amazon AWS S3 service, its regional infrastructure, and its policies. Organizations with strict independence or sovereignty requirements should factor this into their preservation planning.

**Frameworks and standards for reference:**

- [NDSA Levels of Digital Preservation](https://ndsa.org/publications/levels-of-digital-preservation/) — A tiered framework for assessing digital preservation practices
- [Digital Preservation Coalition — Audit and Certification](https://www.dpconline.org/handbook/institutional-strategies/audit-and-certification) — Overview of audit standards and certification options
