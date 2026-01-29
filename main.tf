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

locals {
  stack = random_pet.stack.id
}

resource "random_pet" "stack" {
  length    = 2
  separator = "-"
}

output "stack" {
  value = local.stack
}
