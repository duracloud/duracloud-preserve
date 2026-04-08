# Bucket request policy, permissions and notifications
locals {
  deploy_bucket_request = contains(keys(local.functions), "bucket-request") ? { "bucket-request" = {} } : {}
}

data "aws_iam_policy_document" "bucket_request" {
  for_each = local.deploy_bucket_request

  statement {
    effect = "Allow"
    actions = [
      "s3:GetObject",
      "s3:DeleteObject",
    ]
    resources = ["${aws_s3_bucket.main["request"].arn}/*"]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:PutObject"]
    resources = ["${aws_s3_bucket.main["managed"].arn}/${local.feedback_prefix}/*"]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:CreateBucket",
      "s3:DeleteBucket",
      "s3:DeleteBucketPolicy",
      "s3:GetBucketLogging",
      "s3:GetBucketPublicAccessBlock",
      "s3:GetBucketVersioning",
      "s3:GetInventoryConfiguration",
      "s3:GetLifecycleConfiguration",
      "s3:GetReplicationConfiguration",
      "s3:PutBucketLogging",
      "s3:PutBucketNotification",
      "s3:PutBucketPolicy",
      "s3:PutBucketPublicAccessBlock",
      "s3:PutBucketTagging",
      "s3:PutBucketVersioning",
      "s3:PutInventoryConfiguration",
      "s3:PutLifecycleConfiguration",
      "s3:PutReplicationConfiguration",
      "s3:TagResource",
    ]
    resources = [local.stack_bucket_arn_pattern]
  }

  statement {
    effect    = "Allow"
    actions   = ["iam:PassRole"]
    resources = [aws_iam_role.replication.arn]

    condition {
      test     = "StringEquals"
      variable = "iam:PassedToService"
      values   = ["s3.amazonaws.com"]
    }
  }
}

resource "aws_iam_role_policy" "bucket_request" {
  for_each = local.deploy_bucket_request

  role   = aws_iam_role.lambda[each.key].name
  policy = data.aws_iam_policy_document.bucket_request[each.key].json
}

resource "aws_lambda_permission" "bucket_request" {
  for_each = local.deploy_bucket_request

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main[each.key].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["request"].arn
}

