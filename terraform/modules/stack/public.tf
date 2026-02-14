# Resources for the stack created CloudFront distribution for public file access
locals {
  cert_ready    = var.cert_ready && local.deploy_public
  deploy_public = var.domain != ""
  domain        = var.domain
  fqdn          = "${local.subdomain}.${local.domain}"
  subdomain     = split("-", local.stack)[1]
}

# https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-restricting-access-to-s3.html
resource "aws_s3_bucket_policy" "public" {
  count = local.deploy_public ? 1 : 0

  bucket = aws_s3_bucket.public["public"].id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect    = "Allow"
        Principal = { Service = "cloudfront.amazonaws.com" }
        Action    = ["s3:GetObject"]
        Resource  = "${aws_s3_bucket.public["public"].arn}/*"
        Condition = {
          StringEquals = {
            "AWS:SourceArn" = aws_cloudfront_distribution.public[0].arn
          }
        }
      }
    ]
  })
}

resource "aws_acm_certificate" "public" {
  count = local.deploy_public ? 1 : 0

  provider          = aws.us_east_1
  domain_name       = local.fqdn
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}


resource "aws_cloudfront_origin_access_control" "public" {
  count = local.deploy_public ? 1 : 0

  name                              = local.stack
  origin_access_control_origin_type = "s3"
  signing_behavior                  = "always"
  signing_protocol                  = "sigv4"
}

resource "aws_cloudfront_distribution" "public" {
  count = local.deploy_public ? 1 : 0

  origin {
    domain_name              = aws_s3_bucket.public["public"].bucket_regional_domain_name
    origin_access_control_id = aws_cloudfront_origin_access_control.public[0].id
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
      acm_certificate_arn = aws_acm_certificate.public[0].arn
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

# DNS validation records for another account to create
output "acm_domain_validation_options" {
  value = local.deploy_public ? aws_acm_certificate.public[0].domain_validation_options : null
}

output "cloudfront_domain_name" {
  value = local.deploy_public ? aws_cloudfront_distribution.public[0].domain_name : null
}

output "cloudfront_hosted_zone_id" {
  value = local.deploy_public ? aws_cloudfront_distribution.public[0].hosted_zone_id : null
}
