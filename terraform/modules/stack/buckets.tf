locals {
  bucket_origin = "terraform"
  bucket_type   = "internal"

  # c.f. stack.rs
  buckets = {
    managed = {}
    request = {}
  }
}

resource "aws_s3_bucket" "main" {
  for_each = local.buckets

  bucket        = "${local.stack}-${each.key}"
  force_destroy = true

  # c.f. bucket_creator.rs
  tags = {
    BucketOrigin = local.bucket_origin
    BucketType   = local.bucket_type
    Stack        = local.stack
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
