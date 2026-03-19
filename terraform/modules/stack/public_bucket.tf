locals {
  # "Public" bucket pair for cloudfront: primary + replication target
  public_bucket = {
    noncurrent_days      = 14
    storage_class        = "INTELLIGENT_TIERING"
    transition_days      = 7
    repl_storage_class   = "DEEP_ARCHIVE"
    repl_transition_days = 7
  }
}

# Public bucket pair
resource "aws_s3_bucket" "public" {
  bucket        = "${local.stack}-public"
  force_destroy = true

  tags = {
    BucketOrigin = local.bucket_origin
    BucketType   = "public"
    Stack        = local.stack
  }
}

resource "aws_s3_bucket" "public_repl" {
  bucket        = "${local.stack}-public-repl"
  force_destroy = true

  tags = {
    BucketOrigin = local.bucket_origin
    BucketType   = "replication"
    Stack        = local.stack
  }
}

resource "aws_s3_bucket_versioning" "public" {
  bucket = aws_s3_bucket.public.id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_versioning" "public_repl" {
  bucket = aws_s3_bucket.public_repl.id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "public" {
  bucket = aws_s3_bucket.public.id

  rule {
    id     = "ExpireOldVersions"
    status = "Enabled"

    filter {
      prefix = ""
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 3
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
}

resource "aws_s3_bucket_lifecycle_configuration" "public_repl" {
  bucket = aws_s3_bucket.public_repl.id

  rule {
    id     = "ExpireOldVersions"
    status = "Enabled"

    filter {
      prefix = ""
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 3
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
}

resource "aws_s3_bucket_inventory" "public" {
  bucket = aws_s3_bucket.public.id
  name   = "inventory"

  included_object_versions = "Current"

  schedule {
    frequency = "Daily"
  }

  destination {
    bucket {
      account_id = local.account_id
      bucket_arn = aws_s3_bucket.main["managed"].arn
      format     = "Parquet"
      prefix     = local.inventory_prefix
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
  bucket      = aws_s3_bucket.public.id
  eventbridge = true
}

resource "aws_s3_bucket_replication_configuration" "public" {
  bucket = aws_s3_bucket.public.id
  role   = aws_iam_role.replication.arn

  rule {
    id       = "ReplicateAll"
    status   = "Enabled"
    priority = 1

    filter {
      prefix = ""
    }

    destination {
      bucket = aws_s3_bucket.public_repl.arn

      replication_time {
        status = "Enabled"
        time {
          minutes = 15
        }
      }

      metrics {
        status = "Enabled"
        event_threshold {
          minutes = 15
        }
      }
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
  bucket = aws_s3_bucket.public.id

  target_bucket = aws_s3_bucket.main["managed"].id
  target_prefix = "${local.logging_prefix}/${aws_s3_bucket.public.id}/"
}
