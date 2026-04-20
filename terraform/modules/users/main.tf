terraform {
  required_version = ">= 1.4, < 2.0.0"
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

locals {
  sftpgo_enabled = var.sftpgo_enabled
  users          = var.users
}
