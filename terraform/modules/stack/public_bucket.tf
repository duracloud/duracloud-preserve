locals {
  # "Public" bucket pair for cloudfront: primary + replication target
  public_bucket = {
    noncurrent_days      = local.expire_noncurrent_version_days
    storage_class        = "INTELLIGENT_TIERING"
    transition_days      = local.storage_transition_days
    repl_storage_class   = "DEEP_ARCHIVE"
    repl_transition_days = local.storage_transition_days
  }
}

# Public bucket pair
resource "aws_s3_bucket" "public" {
  for_each = local.deploy_cloudfront

  bucket        = "${local.stack}${local.public_suffix}"
  force_destroy = true

  tags = {
    (local.bucket_tag_origin_key) = local.bucket_origin
    (local.bucket_tag_type_key)   = "public"
    (local.bucket_tag_stack_key)  = local.stack
  }
}

resource "aws_s3_bucket" "public_repl" {
  for_each = local.deploy_cloudfront

  bucket        = "${local.stack}${local.public_suffix}${local.replication_suffix}"
  force_destroy = true

  tags = {
    (local.bucket_tag_origin_key) = local.bucket_origin
    (local.bucket_tag_type_key)   = "replication"
    (local.bucket_tag_stack_key)  = local.stack
  }
}

resource "aws_s3_bucket_versioning" "public" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public[each.key].id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_versioning" "public_repl" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public_repl[each.key].id

  versioning_configuration {
    status = "Enabled"
  }
}

# c.f. bucket_creator.rs
resource "aws_s3_bucket_lifecycle_configuration" "public" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public[each.key].id

  rule {
    id     = "ExpireOldVersions"
    status = "Enabled"

    filter {
      prefix = ""
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = local.expire_aborted_multipart_days
    }

    expiration {
      expired_object_delete_marker = true
    }

    noncurrent_version_expiration {
      noncurrent_days = local.public_bucket.noncurrent_days
    }
  }

  rule {
    id     = local.public_bucket.storage_class
    status = "Enabled"

    filter {
      prefix = ""
    }

    transition {
      days          = local.public_bucket.transition_days
      storage_class = local.public_bucket.storage_class
    }
  }

  rule {
    id     = "ExpireLegacyDuraCloudFiles"
    status = "Enabled"

    filter {
      tag {
        key   = local.lifecycle_legacy_duracloud_file_tag_key
        value = local.lifecycle_legacy_duracloud_file_tag_val
      }
    }

    expiration {
      days = local.expire_legacy_duracloud_file_days
    }
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "public_repl" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public_repl[each.key].id

  rule {
    id     = "ExpireOldVersions"
    status = "Enabled"

    filter {
      prefix = ""
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = local.expire_aborted_multipart_days
    }

    expiration {
      expired_object_delete_marker = true
    }

    noncurrent_version_expiration {
      noncurrent_days = local.public_bucket.noncurrent_days
    }
  }

  rule {
    id     = local.public_bucket.repl_storage_class
    status = "Enabled"

    filter {
      prefix = ""
    }

    transition {
      days          = local.public_bucket.repl_transition_days
      storage_class = local.public_bucket.repl_storage_class
    }
  }

  rule {
    id     = "ExpireLegacyDuraCloudFiles"
    status = "Enabled"

    filter {
      tag {
        key   = local.lifecycle_legacy_duracloud_file_tag_key
        value = local.lifecycle_legacy_duracloud_file_tag_val
      }
    }

    expiration {
      days = local.expire_legacy_duracloud_file_days
    }
  }
}

resource "aws_s3_bucket_inventory" "public" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public[each.key].id
  name   = local.inventory_id

  included_object_versions = "Current"

  schedule {
    frequency = "Daily"
  }

  destination {
    bucket {
      account_id = local.account_id
      bucket_arn = aws_s3_bucket.main["managed"].arn
      format     = "Parquet"
      prefix     = local.manifests_prefix
    }
  }

  optional_fields = [
    "Size",
    "LastModifiedDate",
    "StorageClass",
    "ReplicationStatus",
  ]
}

resource "aws_s3_bucket_notification" "public" {
  for_each = local.deploy_cloudfront

  bucket      = aws_s3_bucket.public[each.key].id
  eventbridge = true
}

resource "aws_s3_bucket_replication_configuration" "public" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public[each.key].id
  role   = aws_iam_role.replication.arn

  rule {
    id       = local.replication_rule_id
    status   = "Enabled"
    priority = local.replication_rule_priority

    filter {
      prefix = ""
    }

    destination {
      bucket = aws_s3_bucket.public_repl[each.key].arn
    }

    delete_marker_replication {
      status = "Enabled"
    }
  }

  depends_on = [
    aws_s3_bucket_versioning.public,
    aws_s3_bucket_versioning.public_repl,
  ]
}

resource "aws_s3_bucket_logging" "public" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public[each.key].id

  target_bucket = aws_s3_bucket.main["managed"].id
  target_prefix = "${local.logging_prefix}/${aws_s3_bucket.public[each.key].id}/"
}

resource "aws_s3_object" "not_found" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public[each.key].id

  key           = "404.txt"
  cache_control = "no-store"
  content_type  = "text/plain"

  content = <<-TEXT
    Not Found

    The requested file was not found. Please check the URL and try again.
  TEXT
}

# Preserve existing resources when enabling conditional public bucket creation.
moved {
  from = aws_s3_bucket.public
  to   = aws_s3_bucket.public["public"]
}

moved {
  from = aws_s3_bucket.public_repl
  to   = aws_s3_bucket.public_repl["public"]
}

moved {
  from = aws_s3_bucket_versioning.public
  to   = aws_s3_bucket_versioning.public["public"]
}

moved {
  from = aws_s3_bucket_versioning.public_repl
  to   = aws_s3_bucket_versioning.public_repl["public"]
}

moved {
  from = aws_s3_bucket_lifecycle_configuration.public
  to   = aws_s3_bucket_lifecycle_configuration.public["public"]
}

moved {
  from = aws_s3_bucket_lifecycle_configuration.public_repl
  to   = aws_s3_bucket_lifecycle_configuration.public_repl["public"]
}

moved {
  from = aws_s3_bucket_inventory.public
  to   = aws_s3_bucket_inventory.public["public"]
}

moved {
  from = aws_s3_bucket_notification.public
  to   = aws_s3_bucket_notification.public["public"]
}

moved {
  from = aws_s3_bucket_replication_configuration.public
  to   = aws_s3_bucket_replication_configuration.public["public"]
}

moved {
  from = aws_s3_bucket_logging.public
  to   = aws_s3_bucket_logging.public["public"]
}

moved {
  from = aws_s3_object.not_found
  to   = aws_s3_object.not_found["public"]
}
