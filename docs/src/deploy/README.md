# Deploying a production stack

This guide shows one way to deploy DuraCloud Preserve using Terraform: a single stack in one AWS account, with bucket management, replication, checksum processing, reports, and access through an S3 client. Adapt the example to your infrastructure conventions and requirements.

## Before you begin

You will need:

- An AWS account, a region, and credentials with permission to deploy the stack's resources, including IAM roles and policies.
- Terraform 1.4 or later, below 2.0, for the stack and users modules used here.
- A place to store Terraform state securely and retain it between deployments. Configure a backend appropriate to your environment; state includes user credentials.
- Release packages in an S3 bucket in the deployment region, accessible to the deployment account. You will need to build and upload these packages or arrange access to published packages. See [Releases](../technical/releases.md) for details.

Choose a stack name such as `dcp-example`. It prefixes resource names, including globally unique S3 bucket names. Use two lowercase alphanumeric parts separated by a hyphen, each at least two characters long and starting with a letter.

## Configure the stack

Create a separate directory for your deployment configuration. Save the following as `main.tf`, replacing `MODULE_COMMIT` with the Git commit you intend to deploy. Use the same revision for both module sources below and choose compatible release packages; module revisions and artifact versions are separate settings.

```hcl
terraform {
  required_version = ">= 1.4, < 2.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }
}

provider "aws" {
  region = var.region
}

variable "region" {
  type    = string
  default = "us-west-2"
}

variable "stack" {
  type    = string
  default = "dcp-example"
}

variable "artifacts_bucket" {
  type = string
}

variable "artifacts_version" {
  type = string
}

locals {
  function_settings = {
    bucket-request = {
      env = { STORAGE_TIER = "INTELLIGENT_TIERING" }
    }
    checksum-request = {
      memory  = 1024
      storage = 2048
      timeout = 600
    }
    checksum-report = {
      memory  = 1024
      storage = 2048
      timeout = 300
    }
    compute-checksums = {
      schedule = "cron(0 0 1 1,6 ? *)"
      timeout  = 120
    }
    inventory-report = {
      memory  = 1024
      storage = 2048
      timeout = 300
    }
    storage-report = {
      schedule = "cron(0 8 ? * MON *)"
      timeout  = 120
    }
  }
}

module "stack" {
  source = "github.com/duracloud/duracloud-preserve//terraform/modules/stack?ref=MODULE_COMMIT"

  stack              = var.stack
  deploy_functions   = true
  cloudfront_enabled = false

  functions = {
    for name, settings in local.function_settings : name => merge(settings, {
      bucket = var.artifacts_bucket
      file   = "v/${var.artifacts_version}/${name}/bootstrap.zip"
    })
  }
}
```

Set your values in `terraform.tfvars`, for example:

```hcl
region            = "us-west-2"
stack             = "dcp-example"
artifacts_bucket  = "your-release-bucket"
artifacts_version = "YOUR_RELEASE_VERSION"
```

The function settings are starting values; adjust memory, temporary storage, timeouts, and schedules for your workload. This example schedules checksum computation on January 1 and June 1 at midnight UTC, and storage reports on Mondays at 08:00 UTC. The other functions respond to events. See [Functions](../technical/functions.md) and [Checksum verification](../checksum-verification.md) for their roles.

The stack creates `${stack}-request` and `${stack}-managed` buckets, along with supporting resources. Content buckets and their replication buckets are created through bucket requests after deployment. CloudFront and its public bucket pair are optional.

## Provide user access

Add the following to `main.tf`, replacing the module revision, username, and email. This example gives one user standard access through an S3 client; see [Users and Groups](../user/users-groups.md) for the available roles.

```hcl
module "users" {
  source = "github.com/duracloud/duracloud-preserve//terraform/modules/users?ref=MODULE_COMMIT"

  sftpgo_enabled = false
  users = {
    archivist = {
      email = "archivist@example.org"
      memberships = [{
        stack = var.stack
        group = "standard-users"
      }]
    }
  }

  depends_on = [module.stack]
}
```

The module stores each user's access key in SSM Parameter Store at `/iam/access_key/<username>` and the secret key as a SecureString at `/iam/secret_key/<username>`. An administrator can retrieve and deliver these credentials to the user for [Connecting to S3](../user/connecting-to-s3.md).

## Apply and check

With your deployment credentials configured, run from the deployment directory:

```bash
terraform init
terraform plan -out=deployment.tfplan
terraform apply deployment.tfplan
```

Review the plan before applying. Keep the configuration and `.terraform.lock.hcl` in version control, and retain the state in your chosen backend.

Connect with the provisioned user's credentials, submit a [bucket request](../user/creating-buckets.md), and check the `feedback` folder in the managed bucket. Upload a sample file to the new content bucket. [Reports](../user/reports.md) and [Checksum verification](../checksum-verification.md) describe how to inspect the resulting inventory and preservation data; these are produced asynchronously.

## Optional additions

### Public access through CloudFront

Set `cloudfront_enabled = true` in the stack module to create a CloudFront distribution, `${stack}-public`, and `${stack}-public-repl`, with their supporting configuration. Add this root output to retrieve the public domain with `terraform output cloudfront_domain_name` after applying:

```hcl
output "cloudfront_domain_name" {
  value = module.stack.cloudfront_domain_name
}
```

To use a custom domain, also pass `cloudfront_domain` and `acm_cert_arn` to the stack module. The certificate must be validated in `us-east-1`. For `dcp-example` and `cloudfront_domain = "preserve.example.org"`, the module uses `example.preserve.example.org`; arrange a certificate covering that name and a DNS record pointing to the distribution. The module exposes `cloudfront_domain_name` and `cloudfront_hosted_zone_id` for DNS configuration.

When CloudFront is disabled, its distribution and bucket pair are absent and its outputs are `null`. The stack module defaults to disabled; this repository's development `main.tf` defaults to enabled. User-created buckets ending in `-public` use direct S3 public access independently of this setting. See [Making Content Public](../user/making-content-public.md).

### Other integrations and settings

- **Notifications and reporting:** set `emails_to_notify` on the stack module for function error emails and confirm the subscription emails. `storage_capacity` sets a reporting reference in bytes, not an enforced quota. See [Storage report](../technical/storage-report.md) for reporting prerequisites.
- **SFTPGo:** provide an existing server and configure its provider, enable `sftpgo_enabled` on the users module, and add the `sync-users` release package to the functions map. See [sync-users](../technical/sync-users.md) for synchronization details.
- **Archive-It:** add the `archive_it` module, pass its `tasks` output to the stack module, and select a published `dcp` image. See the [Archive-It module](https://github.com/duracloud/duracloud-preserve/tree/main/terraform/modules/archive_it) for configuration and [Releases](../technical/releases.md) for image publishing.

## Updating an existing deployment

Change the pinned module revisions and/or `artifacts_version` as needed, then initialize, review the plan, and apply. See [Releases](../technical/releases.md) for selecting older artifacts when rolling back.

When upgrading from unconditional public bucket creation, stacks with CloudFront enabled retain their public resources through Terraform `moved` blocks; no manual state move is needed. With CloudFront disabled, the change plans destruction of any previously created public bucket pair. Disabling CloudFront on an enabled stack also removes its distribution and bucket pair.

Both public buckets have `force_destroy = true`, so applying such a plan can delete their contents, including replicated copies. Preserve any content you need before applying, or keep CloudFront enabled to retain the bucket pair.
