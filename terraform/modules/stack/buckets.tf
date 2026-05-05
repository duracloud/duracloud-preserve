locals {
  bucket_origin = "terraform"
  bucket_type   = "internal"

  # c.f. stack.rs
  buckets = {
    managed = {
      suffix = local.managed_suffix
    }
    request = {
      suffix = local.request_suffix
    }
  }
}

resource "aws_s3_bucket" "main" {
  for_each = local.buckets

  bucket        = "${local.stack}${each.value.suffix}"
  force_destroy = true

  # c.f. bucket_creator.rs
  tags = {
    (local.bucket_tag_origin_key) = local.bucket_origin
    (local.bucket_tag_type_key)   = local.bucket_type
    (local.bucket_tag_stack_key)  = local.stack
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "main" {
  for_each = local.buckets

  bucket = aws_s3_bucket.main[each.key].id

  rule {
    id     = "expire-all"
    status = "Enabled"

    expiration {
      days = 90
    }
  }
}
