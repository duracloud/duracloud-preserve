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
      {
        Sid       = "AllowS3DeliveryFromStack"
        Effect    = "Allow"
        Principal = { Service = ["s3.amazonaws.com", "logging.s3.amazonaws.com"] }
        Action    = "s3:PutObject"
        Resource  = "${aws_s3_bucket.main["managed"].arn}/*"
        Condition = {
          StringEquals = { "aws:SourceAccount" = local.account_id }
          ArnLike      = { "aws:SourceArn" = "arn:aws:s3:::${local.stack}*" }
        }
      },
      {
        Sid       = "AWSCloudTrailAclCheck"
        Effect    = "Allow"
        Principal = { Service = "cloudtrail.amazonaws.com" }
        Action    = "s3:GetBucketAcl"
        Resource  = aws_s3_bucket.main["managed"].arn
        Condition = {
          StringEquals = { "AWS:SourceArn" = local.cloudtrail_arn }
        }
      },
      {
        Sid       = "AWSCloudTrailWrite"
        Effect    = "Allow"
        Principal = { Service = "cloudtrail.amazonaws.com" }
        Action    = "s3:PutObject"
        Resource  = "${aws_s3_bucket.main["managed"].arn}/${local.cloudtrail_prefix}/*"
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

# Managed bucket notifications
resource "aws_s3_bucket_notification" "managed" {
  count = local.deploy_functions ? 1 : 0

  bucket = aws_s3_bucket.main["managed"].id

  lambda_function {
    lambda_function_arn = aws_lambda_function.main["inventory-report"].arn
    events              = ["s3:ObjectCreated:*"]
    filter_prefix       = "${local.inventory_prefix}/"
    filter_suffix       = "manifest.json"
  }

  depends_on = [
    aws_lambda_permission.inventory_report
  ]
}
