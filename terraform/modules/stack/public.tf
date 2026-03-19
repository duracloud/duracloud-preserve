# Resources for the stack created CloudFront distribution for public file access
locals {
  cert_ready           = var.cert_ready && local.deploy_public
  deploy_public        = var.domain != ""
  domain               = var.domain
  fqdn                 = "${local.subdomain}.${local.domain}"
  deploy_public_access = local.deploy_public ? { "public" = {} } : {}
  subdomain            = split("-", local.stack)[1]
}

# https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-restricting-access-to-s3.html
data "aws_iam_policy_document" "public_bucket" {
  for_each = local.deploy_public_access

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
  for_each = local.deploy_public_access

  bucket = aws_s3_bucket.public.id
  policy = data.aws_iam_policy_document.public_bucket[each.key].json
}

resource "aws_acm_certificate" "public" {
  for_each = local.deploy_public_access

  provider          = aws.us_east_1
  domain_name       = local.fqdn
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_cloudfront_origin_access_control" "public" {
  for_each = local.deploy_public_access

  name                              = local.stack
  origin_access_control_origin_type = "s3"
  signing_behavior                  = "always"
  signing_protocol                  = "sigv4"
}

resource "aws_cloudfront_distribution" "public" {
  for_each = local.deploy_public_access

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

  price_class = "PriceClass_100"

  restrictions {
    geo_restriction {
      restriction_type = "whitelist"
      locations        = ["US", "CA"]
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
