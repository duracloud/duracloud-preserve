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
}

module "artifacts" {
  source = "./terraform/modules/artifacts"

  bucket = local.functions_bucket
  files  = { for k, v in local.functions : k => v.file }
}
