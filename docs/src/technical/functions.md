# Functions

The core service functionality is encapsulated by Lambda functions that run on a schedule or in response to S3 events:

| Function | Trigger | Description |
|---|---|---|
| [bucket-request](bucket-request.md) | S3 event | Creates S3 buckets with prefab configuration from an uploaded text file |
| [checksum-report](checksum-report.md) | Scheduled | Compares checksum results across source and replication buckets to detect corruption |
| [compute-checksums](compute-checksums.md) | Scheduled | Triggers S3 batch checksum jobs across all bucket pairs to verify data integrity |
| [inventory-report](inventory-report.md) | S3 event | Processes S3 inventory data into a human-readable CSV manifest and generates storage stats |
| [storage-report](storage-report.md) | Scheduled | Generates an HTML storage usage report across all buckets in the stack |
| [sync-users](sync-users.md) | S3 event | Syncs IAM users to SFTPGo so they can access their stack buckets over SFTP |

All functions can also be run locally via the `dcp` CLI, which additionally provides commands for tasks not covered by Lambda. See [CLI](cli-only.md) for details.
