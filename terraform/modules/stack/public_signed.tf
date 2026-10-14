# Signed URL access and per-path CORS for the public CloudFront distribution.
# The signed cache behavior lives on the distribution in public.tf.
locals {
  signed_enabled = local.cloudfront_enabled && var.cloudfront_signed_enabled
  signed_path    = var.cloudfront_signed_base_path
  signed_keys    = local.cloudfront_enabled ? var.cloudfront_signed_public_keys : {}
  deploy_signed  = length(local.signed_keys) > 0 ? local.deploy_cloudfront : {}
  cors_paths     = local.cloudfront_enabled ? var.cloudfront_cors_allowed_origins_by_path : {}

  # CORS is applied by the response function, which works on every CloudFront
  # pricing plan (response headers policies aren't available on the Free plan).
  # Origins are ASCII, so length() is the size in bytes.
  public_file_response_code = join("", [
    "var CORS_ORIGINS_BY_PATH = ${jsonencode(local.cors_paths)};\n",
    file("${path.module}/assets/cloudfront/public-file-response.js"),
  ])
}

# https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-trusted-signers.html
resource "aws_cloudfront_public_key" "signed" {
  for_each = local.signed_keys

  # Names are unique per account, so a prefix lets a replacement key be
  # created before the old one is destroyed
  name_prefix = "${local.stack}-${each.key}-"
  comment     = "${local.stack} signed URL key ${each.key}"
  encoded_key = each.value

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_cloudfront_key_group" "signed" {
  for_each = local.deploy_signed

  name    = "${local.stack}-signed"
  comment = "Trusted signers for ${local.signed_path}"
  items   = [for key in aws_cloudfront_public_key.signed : key.id]
}
