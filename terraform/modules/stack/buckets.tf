locals {
  bucket_origin = "terraform"
  bucket_type   = "internal"

  # c.f. stack.rs
  buckets = {
    bucket-request = {}
    managed        = {}
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
          ArnLike      = { "aws:SourceArn" = "arn:aws:s3:::${local.stack}*" }
          StringEquals = { "aws:SourceAccount" = local.account_id }
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
