# Instructions

## Optional public access through CloudFront

The stack module requires Terraform 1.1 or later. Set `cloudfront_enabled = true` to create the CloudFront distribution, its `${stack}-public` bucket, and the `${stack}-public-repl` replication bucket. The public bucket's versioning, lifecycle rules, inventory, notifications, replication, logging, and `404.txt` error object are created with this bucket pair.

The stack module defaults to `cloudfront_enabled = false`, which creates neither the distribution nor its bucket pair. The repository's development `main.tf` overrides this default to `true`; set `cloudfront_enabled = false` in `terraform.tfvars` to disable it there. User-created buckets with names ending in `-public` remain independent of this setting and use direct S3 public access.

When enabled, the stack module's `cloudfront_domain_name` output provides the distribution's domain. To use a custom domain, also configure the module's `cloudfront_domain` and `acm_cert_arn` inputs; the certificate must be in `us-east-1`. The module's CloudFront outputs are `null` when CloudFront is disabled.

## Upgrading existing stacks

Existing stacks with CloudFront enabled retain their public bucket resources through Terraform `moved` blocks, which map the previous resource addresses to the conditional instances. No manual state move is needed for this change.

If CloudFront is disabled, applying this change plans destruction of any previously created public bucket pair and its associated resources. Disabling CloudFront on an enabled stack also removes its distribution and bucket pair. Both buckets have `force_destroy = true`, so applying the plan can delete their contents, including replicated copies. Review the plan and preserve any content you need before applying, or enable CloudFront to retain the bucket pair.
