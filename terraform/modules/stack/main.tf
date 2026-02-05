terraform {
  required_version = ">= 1.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }
}

data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

locals {
  account_id        = data.aws_caller_identity.current.account_id
  cloudtrail_prefix = "cloudtrail"
  inventory_prefix  = "manifests" # c.f. bucket_creator.rs
  region            = data.aws_region.current.region
  stack             = var.stack
}
