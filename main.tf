# Development main.tf (this is for internal dev/test only)
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
provider "sftpgo" {}

data "aws_organizations_organization" "current" {}
data "aws_region" "current" {}

variable "archive_it_enabled" { default = false }
variable "archive_it_expiration" { default = null }
variable "cloudfront_domain" { default = "" }
variable "cloudfront_enabled" { default = true }
variable "deploy" { default = false }
variable "emails_to_notify" { default = [] }
variable "sftpgo_enabled" { default = false }
variable "stack" {}
variable "users" { default = {} }

locals {
  deploy         = var.deploy
  org_id         = data.aws_organizations_organization.current.id
  region         = data.aws_region.current.region
  sftpgo_enabled = var.sftpgo_enabled
  stack          = var.stack
  users          = var.users

  functions_bucket = "dcp-artifacts-${local.region}"
  functions = {
    bucket-request = {
      bucket = local.functions_bucket
      file   = "bucket-request/bootstrap.zip"
      env    = { STORAGE_TIER = "INTELLIGENT_TIERING" }
    }
    checksum-report = {
      bucket = local.functions_bucket
      file   = "checksum-report/bootstrap.zip"
    }
    checksum-request = {
      bucket = local.functions_bucket
      file   = "checksum-request/bootstrap.zip"
    }
    compute-checksums = {
      bucket   = local.functions_bucket
      file     = "compute-checksums/bootstrap.zip"
      schedule = "cron(0 8 ? * SUN *)"
    }
    inventory-report = {
      bucket = local.functions_bucket
      file   = "inventory-report/bootstrap.zip"
    }
    storage-report = {
      bucket   = local.functions_bucket
      file     = "storage-report/bootstrap.zip"
      schedule = "cron(0 8 ? * * *)"
    }
    sync-users = {
      bucket = local.functions_bucket
      file   = "sync-users/bootstrap.zip"
    }
  }
}

module "stack" {
  source = "./terraform/modules/stack"

  cloudfront_domain  = var.cloudfront_domain
  cloudfront_enabled = var.cloudfront_enabled
  deploy_functions   = var.deploy
  emails_to_notify   = var.emails_to_notify
  functions          = local.functions
  stack              = local.stack
  storage_capacity   = pow(10, 12) # 1TB
  tasks              = merge(module.archive_it[*].tasks...)
}

module "users" {
  source = "./terraform/modules/users"

  sftpgo_enabled = local.sftpgo_enabled
  users          = local.users

  depends_on = [module.stack]
}

module "archive_it" {
  source = "./terraform/modules/archive_it"

  count = var.archive_it_enabled ? 1 : 0

  stack      = local.stack
  expiration = var.archive_it_expiration

  config = {
    inventory = { schedule = "cron(0 2 * * ? *)" }
    audit     = { schedule = "cron(0 3 * * ? *)" }
    sync      = { schedule = "cron(0 4 * * ? *)" }
  }
}
