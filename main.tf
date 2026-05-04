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

provider "sftpgo" {
  host     = local.sftpgo_host
  username = local.sftpgo_username
  password = local.sftpgo_password
}

data "aws_organizations_organization" "current" {}
data "aws_region" "current" {}

// SFTPGo required params
data "aws_ssm_parameter" "sftpgo_host" {
  name = "/sftpgo/host"
}

data "aws_ssm_parameter" "sftpgo_username" {
  name = "/sftpgo/username"
}

data "aws_ssm_parameter" "sftpgo_password" {
  name            = "/sftpgo/password"
  with_decryption = true
}

variable "cloudfront_domain" { default = "" }
variable "cloudfront_enabled" { default = true }
variable "deploy" { default = false }
variable "sftpgo_enabled" { default = true }
variable "stack" {}
variable "users" { default = {} }

locals {
  deploy = var.deploy
  org_id = data.aws_organizations_organization.current.id
  region = data.aws_region.current.region
  stack  = var.stack
  users  = var.users

  sftpgo_enabled  = var.sftpgo_enabled
  sftpgo_host     = data.aws_ssm_parameter.sftpgo_host.value
  sftpgo_username = data.aws_ssm_parameter.sftpgo_username.value
  sftpgo_password = data.aws_ssm_parameter.sftpgo_password.value

  functions_bucket = "dcp-artifacts-${local.region}"
  functions = {
    bucket-request = {
      bucket = local.functions_bucket
      file   = "bucket-request/bootstrap.zip"
    }
    compute-checksums = {
      bucket = local.functions_bucket
      file   = "compute-checksums/bootstrap.zip"
    }
    checksum-report = {
      bucket = local.functions_bucket
      file   = "checksum-report/bootstrap.zip"
    }
    inventory-report = {
      bucket = local.functions_bucket
      file   = "inventory-report/bootstrap.zip"
    }
    storage-report = {
      bucket = local.functions_bucket
      file   = "storage-report/bootstrap.zip"
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
  stack              = local.stack
  storage_capacity   = pow(10, 12) # 1TB

  functions = local.functions
}

module "users" {
  source = "./terraform/modules/users"

  sftpgo_enabled = local.sftpgo_enabled
  users          = local.users
}

# Outputs for creating the DNS alias record
# (`dev1.example.org` -> `d1234567890abc.cloudfront.net` with zone ID `Z2FDTNDATAQYW2`)
output "cloudfront_domain_name" {
  value = module.stack.cloudfront_domain_name
}

output "cloudfront_hosted_zone_id" {
  value = module.stack.cloudfront_hosted_zone_id
}
