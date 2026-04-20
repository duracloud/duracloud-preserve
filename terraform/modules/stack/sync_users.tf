# Sync users policy, permissions and notifications
locals {
  deploy_sync_users = contains(keys(local.functions), "sync-users") ? { "sync-users" = {} } : {}
}

data "aws_iam_policy_document" "sync_users" {
  for_each = local.deploy_sync_users

  statement {
    effect = "Allow"
    actions = [
      "iam:ListUsers",
      "iam:ListGroupsForUser",
      "iam:ListUserTags",
    ]
    resources = ["*"]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:ListAllMyBuckets",
      "s3:GetBucketTagging",
    ]
    resources = ["*"]
  }

  statement {
    effect  = "Allow"
    actions = ["ssm:GetParameter"]
    resources = [
      "arn:aws:ssm:*:*:parameter${local.user_access_key_namespace}*",
      "arn:aws:ssm:*:*:parameter${local.user_secret_key_namespace}*",
    ]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:DeleteObject"]
    resources = ["${aws_s3_bucket.main["managed"].arn}/${local.sync_users_prefix}/*"]
  }
}

resource "aws_iam_role_policy" "sync_users" {
  for_each = local.deploy_sync_users

  role   = aws_iam_role.lambda[each.key].name
  policy = data.aws_iam_policy_document.sync_users[each.key].json
}

resource "aws_lambda_permission" "sync_users" {
  for_each = local.deploy_sync_users

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main[each.key].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["managed"].arn
}
