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

data "aws_organizations_organization" "current" {}

variable "cloudfront_domain" { default = "" }
variable "cloudfront_enabled" { default = true }
variable "deploy" { default = false }
variable "stack" {}

locals {
  deploy = var.deploy
  org_id = data.aws_organizations_organization.current.id
  stack  = var.stack

  functions_bucket = "artifacts.${local.stack}"
  functions = {
    bucket-request = {
      bucket = local.functions_bucket
      file   = "target/lambda/bucket-request/bootstrap.zip"
      env    = { STORAGE_TIER = "GLACIER_IR" }
    }
    compute-checksums = {
      bucket = local.functions_bucket
      file   = "target/lambda/compute-checksums/bootstrap.zip"
    }
    checksum-report = {
      bucket = local.functions_bucket
      file   = "target/lambda/checksum-report/bootstrap.zip"
    }
    inventory-report = {
      bucket = local.functions_bucket
      file   = "target/lambda/inventory-report/bootstrap.zip"
    }
    storage-report = {
      bucket = local.functions_bucket
      file   = "target/lambda/storage-report/bootstrap.zip"
    }
  }
}

module "stack" {
  source = "./terraform/modules/stack"

  cloudfront_domain  = var.cloudfront_domain
  cloudfront_enabled = var.cloudfront_enabled
  deploy_functions   = var.deploy
  stack              = local.stack
  storage_capacity   = pow(10, 12) # 1TB

  functions = local.functions

  depends_on = [module.artifacts]
}

module "artifacts" {
  source = "./terraform/modules/artifacts"

  bucket = local.functions_bucket
  files  = { for k, v in local.functions : k => v.file }
  org_id = local.org_id
}

# Outputs for creating the DNS alias record
# (`dev1.example.org` -> `d1234567890abc.cloudfront.net` with zone ID `Z2FDTNDATAQYW2`)
output "cloudfront_domain_name" {
  value = module.stack.cloudfront_domain_name
}

output "cloudfront_hosted_zone_id" {
  value = module.stack.cloudfront_hosted_zone_id
}
