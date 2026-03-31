terraform {
  required_version = ">= 1.0"
  required_providers {
    aws = {
      source                = "hashicorp/aws"
      version               = "~> 6.0"
      configuration_aliases = [aws.us_east_1]
    }
  }
}

data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

locals {
  account_id = data.aws_caller_identity.current.account_id
  basic_role = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
  region     = data.aws_region.current.region
  stack      = var.stack

  # tf only
  cloudtrail_prefix = "cloudtrail"

  # Common S3 ARN patterns
  stack_bucket_arn_pattern = "arn:aws:s3:::${local.stack}-*"
  stack_object_arn_pattern = "${local.stack_bucket_arn_pattern}/*"

  managed_bucket_arn        = "arn:aws:s3:::${local.stack}${local.managed_suffix}"
  managed_bucket_object_arn = "${local.managed_bucket_arn}/*"

  repl_bucket_arn_pattern = "arn:aws:s3:::${local.stack}-*${local.replication_suffix}"
  repl_object_arn_pattern = "${local.repl_bucket_arn_pattern}/*"
}
