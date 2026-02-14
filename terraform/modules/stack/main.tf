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

  # prefixes
  batch_prefix      = "batch"      # c.f. batch.rs
  cloudtrail_prefix = "cloudtrail" # tf only
  feedback_prefix   = "feedback"   # c.f. stack.rs
  inventory_prefix  = "manifests"  # c.f. bucket_creator.rs
  logging_prefix    = "audit"      # c.f. bucket_creator.rs
  metadata_prefix   = "metadata"   # c.f. stack.rs
  reports_prefix    = "reports"    # c.f. stack.rs
}
