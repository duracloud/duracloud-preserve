# Generated from shared/constants/src/lib.rs — do not edit.
# Run `make locals` to regenerate.

locals {

  # Prefixes and suffixes
  batch_checksum_prefix     = "batch/reports/checksum"
  batch_manifest_prefix     = "batch/manifests"
  batch_policy_suffix       = "-s3-batch-policy"
  batch_report_prefix       = "batch/reports"
  batch_role_suffix         = "-s3-batch-role"
  bucket_request_prefix     = "buckets"
  checksum_request_prefix   = "checksums"
  feedback_prefix           = "feedback"
  logging_prefix            = "audit"
  managed_suffix            = "-managed"
  manifests_prefix          = "manifests"
  metadata_prefix           = "metadata"
  public_suffix             = "-public"
  replication_policy_suffix = "-s3-replication-policy"
  replication_role_suffix   = "-s3-replication-role"
  replication_suffix        = "-repl"
  reports_prefix            = "reports"
  request_suffix            = "-request"
  storage_capacity_suffix   = "-storage-capacity"
  sync_users_prefix         = "sync-users"

  # Inventory
  inventory_id = "inventory"

  # Bucket naming rules
  bucket_name_min_parts  = 3
  stack_bucket_delimiter = "-"

  # Bucket request rules
  max_buckets_per_request = 5
  max_request_file_size   = 512
  max_len_for_name        = 63

  # Bucket tagging
  bucket_tag_origin_key                   = "BucketOrigin"
  bucket_tag_origin_val                   = "bucket-request"
  bucket_tag_stack_key                    = "Stack"
  bucket_tag_transition_storage_class_key = "TransitionStorageClass"
  bucket_tag_type_key                     = "BucketType"

  # Bucket lifecycle
  expire_aborted_multipart_days           = 3
  expire_legacy_duracloud_file_days       = 3
  expire_noncurrent_version_days          = 14
  lifecycle_legacy_duracloud_file_tag_key = "ObsoleteDuraCloudFile"
  lifecycle_legacy_duracloud_file_tag_val = "true"
  storage_transition_days                 = 3

  # Bucket replication
  replication_rule_id       = "ReplicateAll"
  replication_rule_priority = 1
  replication_time_minutes  = 15

  # Content types
  application_json = "application/json"
  text_csv         = "text/csv"
  text_html        = "text/html"
  text_plain       = "text/plain"
  text_xml         = "text/xml"

  # Users
  sftpgo_namespace          = "/sftpgo/"
  sync_users_file           = "TRIGGER"
  user_access_key_namespace = "/iam/access_key/"
  user_secret_key_namespace = "/iam/secret_key/"
}
