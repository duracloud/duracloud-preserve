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
  account_id       = data.aws_caller_identity.current.account_id
  basic_role       = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
  name             = var.name
  region           = data.aws_region.current.region
  stack            = var.stack
  storage_capacity = var.storage_capacity

  # names shared with rust code (stack.rs)
  batch_role_name             = "${local.stack}-s3-batch-role"
  replication_role_name       = "${local.stack}-s3-replication-role"
  storage_capacity_param_name = "${local.stack}-storage-capacity"

  # prefixes and suffixes shared with rust code (stack.rs)
  batch_prefix      = "batch"
  cloudtrail_prefix = "cloudtrail" # tf only
  feedback_prefix   = "feedback"
  inventory_prefix  = "manifests"
  logging_prefix    = "audit"
  managed_suffix    = "managed"
  metadata_prefix   = "metadata"
  repl_suffix       = "repl"
  reports_prefix    = "reports"

  # Common S3 ARN patterns
  stack_bucket_arn_pattern = "arn:aws:s3:::${local.stack}-*"
  stack_object_arn_pattern = "${local.stack_bucket_arn_pattern}/*"

  managed_bucket_arn        = "arn:aws:s3:::${local.stack}-${local.managed_suffix}"
  managed_bucket_object_arn = "${local.managed_bucket_arn}/*"

  repl_bucket_arn_pattern = "arn:aws:s3:::${local.stack}-*-${local.repl_suffix}"
  repl_object_arn_pattern = "${local.repl_bucket_arn_pattern}/*"
}
