# Development main.tf (this is for dev/test only)
# See the documentation for production deployment instructions
terraform {
  backend "local" {}

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }
}

provider "aws" {}
variable "deploy" { default = false }
variable "stack" {}

# To one-off run compute checksums create a `.local.auto.tfvars` with content like:
# compute_checksums_schedule = "at(2026-02-10T16:00:00)"
# compute_checksums_tz       = "America/Los_Angeles"
# Deploy it, then delete when done.
variable "compute_checksums_schedule" { default = null }
variable "compute_checksums_tz" { default = null }

locals {
  deploy = var.deploy
  stack  = var.stack

  functions_bucket = "artifacts.${local.stack}"
  functions = {
    bucket-request = {
      bucket = local.functions_bucket
      file   = "target/lambda/bucket-request/bootstrap.zip"
      env    = { STORAGE_TIER = "GLACIER_IR" }
    }
    compute-checksums = merge(
      {
        bucket = local.functions_bucket
        file   = "target/lambda/compute-checksums/bootstrap.zip"
      },
      var.compute_checksums_schedule != null ? {
        schedule = var.compute_checksums_schedule
        tz       = coalesce(var.compute_checksums_tz, "America/Los_Angeles")
      } : {}
    )
    inventory-report = {
      bucket = local.functions_bucket
      file   = "target/lambda/inventory-report/bootstrap.zip"
    }
  }
}

module "stack" {
  source = "./terraform/modules/stack"

  deploy_functions = var.deploy
  stack            = local.stack
  functions        = local.functions

  depends_on = [module.artifacts]
}

module "artifacts" {
  source = "./terraform/modules/artifacts"

  bucket = local.functions_bucket
  files  = { for k, v in local.functions : k => v.file }
}
