# Resources for the stack created CloudFront distribution for public file access
locals {
  custom_domain                   = local.cloudfront_enabled && var.cloudfront_domain != ""
  cert_ready                      = local.custom_domain && var.cert_ready
  cloudfront_enabled              = var.cloudfront_enabled
  cloudfront_geo_restriction_list = var.cloudfront_geo_restriction_list
  cloudfront_geo_restriction_type = var.cloudfront_geo_restriction_type
  cloudfront_price_class          = var.cloudfront_price_class
  deploy_cloudfront               = local.cloudfront_enabled ? { "public" = {} } : {}
  deploy_acm                      = local.custom_domain ? { "public" = {} } : {}
  fqdn                            = "${local.subdomain}.${var.cloudfront_domain}"
  subdomain                       = split("-", local.stack)[1]
}

# https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-restricting-access-to-s3.html
data "aws_iam_policy_document" "public_bucket" {
  for_each = local.deploy_cloudfront

  statement {
    effect    = "Allow"
    actions   = ["s3:GetObject"]
    resources = ["${aws_s3_bucket.public.arn}/*"]

    principals {
      type        = "Service"
      identifiers = ["cloudfront.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "AWS:SourceArn"
      values   = [aws_cloudfront_distribution.public[each.key].arn]
    }
  }
}

resource "aws_s3_bucket_policy" "public" {
  for_each = local.deploy_cloudfront

  bucket = aws_s3_bucket.public.id
  policy = data.aws_iam_policy_document.public_bucket[each.key].json
}

resource "aws_acm_certificate" "public" {
  for_each = local.deploy_acm

  provider          = aws.us_east_1
  domain_name       = local.fqdn
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_cloudfront_origin_access_control" "public" {
  for_each = local.deploy_cloudfront

  name                              = local.stack
  origin_access_control_origin_type = "s3"
  signing_behavior                  = "always"
  signing_protocol                  = "sigv4"
}

resource "aws_cloudfront_distribution" "public" {
  for_each = local.deploy_cloudfront

  origin {
    domain_name              = aws_s3_bucket.public.bucket_regional_domain_name
    origin_access_control_id = aws_cloudfront_origin_access_control.public[each.key].id
    origin_id                = local.stack
  }

  aliases         = local.cert_ready ? [local.fqdn] : []
  enabled         = true
  is_ipv6_enabled = true

  default_cache_behavior {
    allowed_methods  = ["GET", "HEAD", "OPTIONS"]
    cached_methods   = ["GET", "HEAD", "OPTIONS"]
    target_origin_id = local.stack

    forwarded_values {
      query_string = false

      cookies {
        forward = "none"
      }
    }

    min_ttl                = 0
    default_ttl            = 3600
    max_ttl                = 86400
    compress               = true
    viewer_protocol_policy = "redirect-to-https"
  }

  price_class = local.cloudfront_price_class

  restrictions {
    geo_restriction {
      restriction_type = local.cloudfront_geo_restriction_type
      locations        = local.cloudfront_geo_restriction_list
    }
  }

  dynamic "viewer_certificate" {
    for_each = local.cert_ready ? [1] : []
    content {
      acm_certificate_arn = aws_acm_certificate.public[each.key].arn
      ssl_support_method  = "sni-only"
    }
  }

  dynamic "viewer_certificate" {
    for_each = local.cert_ready ? [] : [1]
    content {
      cloudfront_default_certificate = true
    }
  }
}
