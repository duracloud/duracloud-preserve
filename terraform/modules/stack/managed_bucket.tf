# Managed bucket policy resources
locals {
  # Constructed this way rather than via resource arn to break circular dependency
  cloudtrail_arn = "arn:aws:cloudtrail:${local.region}:${local.account_id}:trail/${local.stack}-cloudtrail"
}

data "aws_iam_policy_document" "managed_bucket" {
  # S3 Inventory -> inventory prefix
  statement {
    sid     = "AllowS3InventoryFromStack"
    effect  = "Allow"
    actions = ["s3:PutObject"]
    resources = [
      "${aws_s3_bucket.main["managed"].arn}/${local.manifests_prefix}/*"
    ]

    principals {
      type        = "Service"
      identifiers = ["s3.amazonaws.com"]
    }

    condition {
      test     = "ArnLike"
      variable = "aws:SourceArn"
      values   = [local.stack_bucket_arn_pattern]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [local.account_id]
    }

    condition {
      test     = "StringEquals"
      variable = "s3:x-amz-acl"
      values   = ["bucket-owner-full-control"]
    }
  }

  # S3 Server Access Logs -> logging prefix
  statement {
    sid     = "AllowS3ServerAccessLogsFromStack"
    effect  = "Allow"
    actions = ["s3:PutObject"]
    resources = [
      "${aws_s3_bucket.main["managed"].arn}/${local.logging_prefix}/*"
    ]

    principals {
      type        = "Service"
      identifiers = ["logging.s3.amazonaws.com"]
    }

    condition {
      test     = "ArnLike"
      variable = "aws:SourceArn"
      values   = [local.stack_bucket_arn_pattern]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [local.account_id]
    }
  }

  # CloudTrail checks bucket ACL
  statement {
    sid       = "AWSCloudTrailAclCheck"
    effect    = "Allow"
    actions   = ["s3:GetBucketAcl"]
    resources = [aws_s3_bucket.main["managed"].arn]

    principals {
      type        = "Service"
      identifiers = ["cloudtrail.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [local.account_id]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceArn"
      values   = [local.cloudtrail_arn]
    }
  }

  # CloudTrail writes logs -> cloudtrail prefix
  statement {
    sid     = "AWSCloudTrailWrite"
    effect  = "Allow"
    actions = ["s3:PutObject"]
    resources = [
      "${aws_s3_bucket.main["managed"].arn}/${local.cloudtrail_prefix}/*"
    ]

    principals {
      type        = "Service"
      identifiers = ["cloudtrail.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [local.account_id]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceArn"
      values   = [local.cloudtrail_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "s3:x-amz-acl"
      values   = ["bucket-owner-full-control"]
    }
  }
}

# Managed bucket policy
resource "aws_s3_bucket_policy" "managed" {
  bucket = aws_s3_bucket.main["managed"].id
  policy = data.aws_iam_policy_document.managed_bucket.json
}
