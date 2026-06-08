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
  bucket        = "${local.stack}${local.public_suffix}"
  force_destroy = true

  tags = {
    (local.bucket_tag_origin_key) = local.bucket_origin
    (local.bucket_tag_type_key)   = "public"
    (local.bucket_tag_stack_key)  = local.stack
  }
}

resource "aws_s3_bucket" "public_repl" {
  bucket        = "${local.stack}${local.public_suffix}${local.replication_suffix}"
  force_destroy = true

  tags = {
    (local.bucket_tag_origin_key) = local.bucket_origin
    (local.bucket_tag_type_key)   = "replication"
    (local.bucket_tag_stack_key)  = local.stack
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
}

resource "aws_s3_bucket_inventory" "public" {
  bucket = aws_s3_bucket.public.id
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
  bucket      = aws_s3_bucket.public.id
  eventbridge = true
}

resource "aws_s3_bucket_replication_configuration" "public" {
  bucket = aws_s3_bucket.public.id
  role   = aws_iam_role.replication.arn

  rule {
    id       = local.replication_rule_id
    status   = "Enabled"
    priority = local.replication_rule_priority

    filter {
      prefix = ""
    }

    destination {
      bucket = aws_s3_bucket.public_repl.arn

      replication_time {
        status = "Enabled"
        time {
          minutes = local.replication_time_minutes
        }
      }

      metrics {
        status = "Enabled"
        event_threshold {
          minutes = local.replication_time_minutes
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

resource "aws_s3_object" "not_found" {
  bucket = aws_s3_bucket.public.id

  key          = "404.html"
  content_type = "text/html"

  content = <<-HTML
    <!doctype html>
    <html>
    <head>
      <title>404</title>
    </head>
    <body>
      <h1>Not Found</h1>
      <p>The requested file was not found.</p>
      <p>Please check the URL and try again.</p>
    </body>
    </html>
  HTML
}
