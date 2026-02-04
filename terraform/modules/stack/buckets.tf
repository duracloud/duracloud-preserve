locals {
  buckets = {
    bucket-request = {}
    cloudtrail     = {}
    managed        = {}
  }

  # Constructed this way rather than via resource arn to break circular dependency
  cloudtrail_arn = "arn:aws:cloudtrail:${local.region}:${local.account_id}:trail/${local.stack}-cloudtrail"
}

resource "aws_s3_bucket" "main" {
  for_each = local.buckets

  bucket = "${local.stack}-${each.key}"
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

# CloudTrail bucket policy
resource "aws_s3_bucket_policy" "cloudtrail" {
  bucket = aws_s3_bucket.main["cloudtrail"].id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AWSCloudTrailAclCheck"
        Effect = "Allow"
        Principal = {
          Service = "cloudtrail.amazonaws.com"
        }
        Action   = "s3:GetBucketAcl"
        Resource = aws_s3_bucket.main["cloudtrail"].arn
        Condition = {
          StringEquals = {
            "AWS:SourceArn" = local.cloudtrail_arn
          }
        }
      },
      {
        Sid    = "AWSCloudTrailWrite"
        Effect = "Allow"
        Principal = {
          Service = "cloudtrail.amazonaws.com"
        }
        Action   = "s3:PutObject"
        Resource = "${aws_s3_bucket.main["cloudtrail"].arn}/*"
        Condition = {
          StringEquals = {
            "s3:x-amz-acl"  = "bucket-owner-full-control"
            "AWS:SourceArn" = local.cloudtrail_arn
          }
        }
      }
    ]
  })
}

# Managed bucket policy
resource "aws_s3_bucket_policy" "managed" {
  bucket = aws_s3_bucket.main["managed"].id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid       = "AllowS3DeliveryFromStack"
      Effect    = "Allow"
      Principal = { Service = ["s3.amazonaws.com", "logging.s3.amazonaws.com"] }
      Action    = "s3:PutObject"
      Resource  = "${aws_s3_bucket.main["managed"].arn}/*"
      Condition = {
        StringEquals = { "aws:SourceAccount" = local.account_id }
        ArnLike      = { "aws:SourceArn" = "arn:aws:s3:::${local.stack}*" }
      }
    }]
  })
}
