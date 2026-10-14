# Signed URL access and per-path CORS for the public CloudFront distribution.
# The ordered cache behaviors that use these live on the distribution in public.tf.
locals {
  signed_enabled = local.cloudfront_enabled && var.cloudfront_signed_enabled
  signed_path    = var.cloudfront_signed_base_path
  signed_keys    = local.cloudfront_enabled ? var.cloudfront_signed_public_keys : {}
  deploy_signed  = length(local.signed_keys) > 0 ? local.deploy_cloudfront : {}
  cors_paths     = local.cloudfront_enabled ? var.cloudfront_cors_allowed_origins_by_path : {}

  # CORS paths that get their own unsigned behavior (the signed path's entry
  # attaches to the signed behavior instead)
  cors_public_paths = {
    for path, origins in local.cors_paths : path => origins
    if !(local.signed_enabled && path == local.signed_path)
  }
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

resource "aws_cloudfront_response_headers_policy" "cors" {
  for_each = local.cors_paths

  # Path slug for readability plus a hash so distinct paths can't collide
  name    = "${substr("${local.stack}-cors-${trim(replace(each.key, "/", "-"), "-")}", 0, 117)}-${substr(sha1(each.key), 0, 10)}"
  comment = "CORS for ${each.key}"

  cors_config {
    access_control_allow_credentials = false
    origin_override                  = true

    # Required by the API; players must not send preflights (see docs)
    access_control_allow_headers {
      items = ["Range"]
    }

    access_control_allow_methods {
      items = ["GET", "HEAD"]
    }

    access_control_allow_origins {
      items = each.value
    }

    access_control_expose_headers {
      items = ["Accept-Ranges", "Content-Range", "ETag"]
    }
  }
}
