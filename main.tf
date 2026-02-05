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

  # functions = {
  #   bucket-request = {}
  # }

  function_bucket = "artifacts.${local.stack}"
  function_files = {
    bucket-request = "target/lambda/bucket-request/bootstrap.zip"
  }
}

module "stack" {
  source = "./terraform/modules/stack"

  deploy_functions = var.deploy
  stack            = local.stack

  function_bucket = local.function_bucket
  function_files  = local.function_files
}

module "artifacts" {
  source = "./terraform/modules/artifacts"

  bucket = local.function_bucket
  files  = local.function_files
}
