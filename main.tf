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
      source  = "hashicorp/random"
      version = "~> 3.0"
    }
  }
}

provider "aws" {}

provider "aws" {
  alias  = "us_east_1"
  region = "us-east-1" # for cloudfront
}

resource "random_pet" "name" {
  count = var.name == null ? 1 : 0
}

variable "deploy" { default = false }
variable "domain" { default = "duracloud.org" } # omit or "" for no cloudfront resources
variable "name" { default = null }
variable "stack" {}

locals {
  name = coalesce(var.name, one(random_pet.name[*].id))
}

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

  providers = {
    aws           = aws
    aws.us_east_1 = aws.us_east_1
  }

  deploy_functions = var.deploy
  domain           = var.domain
  name             = local.name
  stack            = local.stack
  storage_capacity = pow(10, 12) # 1TB

  functions = local.functions

  depends_on = [module.artifacts]
}

module "artifacts" {
  source = "./terraform/modules/artifacts"

  bucket = local.functions_bucket
  files  = { for k, v in local.functions : k => v.file }
}

# Outputs for creating DNS records:
# 1. ACM validation CNAME (`_abc123.dev1.example.org` -> `_def456.acm-validations.aws.`)
# 2. Alias record (`dev1.example.org` -> `d1234567890abc.cloudfront.net` with zone ID `Z2FDTNDATAQYW2`)
output "acm_domain_validation_options" {
  value = module.stack.acm_domain_validation_options
}

output "cloudfront_domain_name" {
  value = module.stack.cloudfront_domain_name
}

output "cloudfront_hosted_zone_id" {
  value = module.stack.cloudfront_hosted_zone_id
}
