# Development main.tf (this is for dev/test only)
# See the documentation for production deployment instructions
terraform {
  backend "local" {}

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }

    random = {
      source = "hashicorp/random"
    }
  }
}

provider "aws" {}

data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

locals {
  stack      = random_pet.stack.id
  account_id = data.aws_caller_identity.current.account_id
  region     = data.aws_region.current.name
}

resource "random_pet" "stack" {
  length    = 2
  separator = "-"
}

output "stack" {
  value = local.stack
}

# CloudTrail for S3 Batch Operations events
resource "aws_s3_bucket" "cloudtrail" {
  bucket = "${local.stack}-cloudtrail"
}

resource "aws_s3_bucket_policy" "cloudtrail" {
  bucket = aws_s3_bucket.cloudtrail.id
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
        Resource = aws_s3_bucket.cloudtrail.arn
        Condition = {
          StringEquals = {
            "AWS:SourceArn" = "arn:aws:cloudtrail:${local.region}:${local.account_id}:trail/${local.stack}-trail"
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
        Resource = "${aws_s3_bucket.cloudtrail.arn}/*"
        Condition = {
          StringEquals = {
            "s3:x-amz-acl"  = "bucket-owner-full-control"
            "AWS:SourceArn" = "arn:aws:cloudtrail:${local.region}:${local.account_id}:trail/${local.stack}-trail"
          }
        }
      }
    ]
  })
}

resource "aws_cloudtrail" "main" {
  name                          = "${local.stack}-trail"
  s3_bucket_name                = aws_s3_bucket.cloudtrail.id
  include_global_service_events = false
  is_multi_region_trail         = false

  event_selector {
    read_write_type           = "All"
    include_management_events = true
  }

  depends_on = [aws_s3_bucket_policy.cloudtrail]
}

# EventBridge rule for S3 Batch Operations job completion
resource "aws_cloudwatch_event_rule" "batch_job_complete" {
  name        = "${local.stack}-batch-job-complete"
  description = "Triggers on S3 Batch Operations job completion or failure"

  event_pattern = jsonencode({
    source      = ["aws.s3"]
    detail-type = ["AWS Service Event via CloudTrail"]
    detail = {
      eventSource = ["s3.amazonaws.com"]
      eventName   = ["JobStatusChanged"]
      serviceEventDetails = {
        status = ["Complete", "Failed"]
      }
    }
  })
}

# Target will be added when Lambda is deployed
# Note multiple functions may be interested in these events
# so each function must determine whether the job id is in scope to handle
# resource "aws_cloudwatch_event_target" "checksum_report" {
#   rule      = aws_cloudwatch_event_rule.batch_job_complete.name
#   target_id = "checksum-report"
#   arn       = aws_lambda_function.checksum_report.arn
# }
