# Managed Resources
When you view your S3 account using a GUI client or the AWS CLI for the first time, you will notice a number of pre-existing buckets that have been created.

## Pre-Existing Buckets
- `duracloud-$ID-bucket-request`: Used to make requests to create new buckets.
  See: https://wiki.lyrasis.org/display/D2/How+to+Create+Buckets
- `duracloud-$ID-managed`: Used to deposit generated files such as audit history, exports, inventory, and reports.
  **This bucket is read-only.**
- `duracloud-$ID-private`: Default private bucket.
- `duracloud-$ID-public`: Default public bucket. Files uploaded here will have a publicly accessible URL.

## Managed Bucket Structure
Over time, the `duracloud-$ID-managed` bucket will contain the following prefixes (folders):
- `audit`: AWS generated Audit logs
- `batch`: AWS generated files related to S3 batch operations
- `cloudtrail`: AWS generated files for events related to S3
- `feedback`: Application generated files for troubleshooting issues
- `manifests`: AWS generated inventory files
- `metadata`: Application generated files related to various stats (checksum, usage etc.)
- `reports`: Application generated files intended for user review and download

More information about the data available in the `-managed` bucket is available on the **Reports** page.

> [!Tip]
> - If the AWS account is used for purposes, additional buckets may exist. This may also occur if there are multiple stacks per account.
> - However, the access credentials provided for this service will only work with the eligible stack resources associated with the user credentials.
