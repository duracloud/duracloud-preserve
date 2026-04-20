# Development main.tf (this is for dev/test only)
# See the documentation for production deployment instructions
terraform {
  backend "local" {}

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
    sftpgo = {
      source  = "drakkan/sftpgo"
      version = "~> 0.0.22"
    }
  }
}

provider "aws" {}

provider "sftpgo" {
  host     = local.sftpgo_host
  username = local.sftpgo_username
  password = local.sftpgo_password
}

data "aws_organizations_organization" "current" {}

// If not using SFTPGo then create the param with placeholder value
data "aws_ssm_parameter" "sftpgo_password" {
  name            = "/sftpgo/password"
  with_decryption = true
}

variable "cloudfront_domain" { default = "" }
variable "cloudfront_enabled" { default = true }
variable "deploy" { default = false }
variable "sftpgo_host" { default = null }
variable "stack" {}
variable "users" { default = {} }

locals {
  deploy          = var.deploy
  org_id          = data.aws_organizations_organization.current.id
  sftpgo_host     = var.sftpgo_host
  sftpgo_password = data.aws_ssm_parameter.sftpgo_password.value
  sftpgo_username = "admin"
  stack           = var.stack
  users           = var.users

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
    sync-users = {
      bucket = local.functions_bucket
      file   = "target/lambda/sync-users/bootstrap.zip"
      env = {
        SFTPGO_HOST     = local.sftpgo_host
        SFTPGO_USERNAME = local.sftpgo_username
        SFTPGO_PASSWORD = local.sftpgo_password
      }
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

module "users" {
  source = "./terraform/modules/users"

  sftpgo_enabled = local.sftpgo_host != null
  users          = local.users
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
