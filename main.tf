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
      source = "hashicorp/random"
    }
  }
}

provider "aws" {}
variable "stack" {}

locals {
  stack = var.stack
}

module "stack" {
  source = "./terraform/modules/stack"

  stack = local.stack
}
