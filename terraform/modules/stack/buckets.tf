locals {
  bucket_origin = "terraform"
  bucket_type   = "internal"

  # c.f. stack.rs
  buckets = {
    bucket-request = {}
    managed        = {}
  }

  public_buckets = {
    public = {
      bucket_type     = "public"
      storage_class   = "INTELLIGENT_TIERING"
      transition_days = 7
    }
    public-repl = {
      bucket_type     = "replication"
      storage_class   = "DEEP_ARCHIVE"
      transition_days = 7
    }
  }

  # Constructed this way rather than via resource arn to break circular dependency
  cloudtrail_arn = "arn:aws:cloudtrail:${local.region}:${local.account_id}:trail/${local.stack}-cloudtrail"
}

resource "aws_s3_bucket" "main" {
  for_each = local.buckets

  bucket        = "${local.stack}-${each.key}"
  force_destroy = true

  # c.f. bucket_creator.rs
  tags = {
    BucketOrigin = local.bucket_origin
    BucketType   = local.bucket_type
    Stack        = local.stack
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "main" {
  for_each = local.buckets

  bucket = aws_s3_bucket.main[each.key].id

  rule {
    id     = "expire-all"
    status = "Enabled"

    expiration {
      days = 90
    }
  }
}

# Managed bucket policy
resource "aws_s3_bucket_policy" "managed" {
  bucket = aws_s3_bucket.main["managed"].id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      # S3 Inventory -> inventory prefix
      {
        Sid       = "AllowS3InventoryFromStack"
        Effect    = "Allow"
        Principal = { Service = "s3.amazonaws.com" }
        Action    = "s3:PutObject"
        Resource  = "${aws_s3_bucket.main["managed"].arn}/${local.inventory_prefix}/*"
        Condition = {
          ArnLike = { "aws:SourceArn" = "arn:aws:s3:::${local.stack}*" }
          StringEquals = {
            "aws:SourceAccount" = local.account_id
            "s3:x-amz-acl"      = "bucket-owner-full-control"
          }
        }
      },
      # S3 Server Access Logs -> logging prefix
      {
        Sid       = "AllowS3ServerAccessLogsFromStack"
        Effect    = "Allow"
        Principal = { Service = "logging.s3.amazonaws.com" }
        Action    = "s3:PutObject"
        Resource  = "${aws_s3_bucket.main["managed"].arn}/${local.logging_prefix}/*"
        Condition = {
          ArnLike      = { "aws:SourceArn" = "arn:aws:s3:::${local.stack}*" }
          StringEquals = { "aws:SourceAccount" = local.account_id }
        }
      },
      # CloudTrail checks bucket ACL
      {
        Sid       = "AWSCloudTrailAclCheck"
        Effect    = "Allow"
        Principal = { Service = "cloudtrail.amazonaws.com" }
        Action    = "s3:GetBucketAcl"
        Resource  = aws_s3_bucket.main["managed"].arn
        Condition = {
          StringEquals = {
            "aws:SourceAccount" = local.account_id
            "aws:SourceArn"     = local.cloudtrail_arn
          }
        }
      },
      # CloudTrail writes logs -> cloudtrail prefix
      {
        Sid       = "AWSCloudTrailWrite"
        Effect    = "Allow"
        Principal = { Service = "cloudtrail.amazonaws.com" }
        Action    = "s3:PutObject"
        Resource  = "${aws_s3_bucket.main["managed"].arn}/${local.cloudtrail_prefix}/*"
        Condition = {
          StringEquals = {
            "aws:SourceAccount" = local.account_id
            "aws:SourceArn"     = local.cloudtrail_arn
            "s3:x-amz-acl"      = "bucket-owner-full-control"
          }
        }
      }
    ]
  })
}

# Stack created public buckets
resource "aws_s3_bucket" "public" {
  for_each = local.public_buckets

  bucket        = "${local.stack}-${each.key}"
  force_destroy = true

  tags = {
    BucketOrigin = local.bucket_origin
    BucketType   = each.value.bucket_type
    Stack        = local.stack
  }
}

resource "aws_s3_bucket_versioning" "public" {
  for_each = local.public_buckets

  bucket = aws_s3_bucket.public[each.key].id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "public" {
  for_each = local.public_buckets

  bucket = aws_s3_bucket.public[each.key].id

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
      noncurrent_days = 14
    }
  }

  rule {
    id     = each.value.storage_class
    status = "Enabled"

    filter {
      prefix = ""
    }

    transition {
      days          = each.value.transition_days
      storage_class = each.value.storage_class
    }
  }
}

resource "aws_s3_bucket_inventory" "public" {
  for_each = { for k, v in local.public_buckets : k => v if v.bucket_type == "public" }

  bucket = aws_s3_bucket.public[each.key].id
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
  for_each = { for k, v in local.public_buckets : k => v if v.bucket_type == "public" }

  bucket      = aws_s3_bucket.public[each.key].id
  eventbridge = true
}

resource "aws_s3_bucket_replication_configuration" "public" {
  bucket = aws_s3_bucket.public["public"].id
  role   = aws_iam_role.replication.arn

  rule {
    id       = "ReplicateAll"
    status   = "Enabled"
    priority = 1

    filter {
      prefix = ""
    }

    destination {
      bucket = aws_s3_bucket.public["public-repl"].arn

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
    aws_s3_bucket_versioning.public["public"],
    aws_s3_bucket_versioning.public["public-repl"],
  ]
}

resource "aws_s3_bucket_logging" "public" {
  bucket = aws_s3_bucket.public["public"].id

  target_bucket = aws_s3_bucket.main["managed"].id
  target_prefix = "${local.logging_prefix}/${aws_s3_bucket.public["public"].id}/"
}
