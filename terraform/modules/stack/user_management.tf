# User groups and policies
locals {
  user_group_policies = {
    power_users = {
      group_name  = "${local.stack}-power-users"
      policy_name = "${local.stack}-power-user-policy"
      description = "Policy for power users"
      # CRUD on user created buckets
      allow_actions = [
        "s3:ListBucket",
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts",
        "s3:ListBucketMultipartUploads",
      ]
    }
    standard_users = {
      group_name  = "${local.stack}-standard-users"
      policy_name = "${local.stack}-standard-users-policy"
      description = "Policy for standard users"
      # Can download and upload to but not delete from user created buckets
      allow_actions = [
        "s3:ListBucket",
        "s3:GetObject",
        "s3:PutObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts",
        "s3:ListBucketMultipartUploads",
      ]
    }
  }

  # Scoped stack buckets (including bucket-request)
  stack_bucket_resources = [
    local.stack_bucket_arn_pattern,
    local.stack_object_arn_pattern,
  ]

  managed_bucket_resources = [
    local.managed_bucket_arn,
    local.managed_bucket_object_arn,
  ]

  # Actions permitted on managed buckets (read-only)
  managed_bucket_permitted = ["s3:ListBucket", "s3:GetObject"]

  repl_bucket_resources = [
    local.repl_bucket_arn_pattern,
    local.repl_object_arn_pattern,
  ]

  # Actions permitted on repl buckets (list-only)
  # Cannot upload to, delete from or download from repl buckets (listing is ok)
  repl_bucket_permitted = ["s3:ListBucket"]
}

data "aws_iam_policy_document" "user_groups" {
  for_each = local.user_group_policies

  statement {
    effect    = "Allow"
    actions   = ["s3:ListAllMyBuckets"]
    resources = ["*"]
  }

  statement {
    effect    = "Allow"
    actions   = each.value.allow_actions
    resources = local.stack_bucket_resources
  }

  statement {
    effect    = "Deny"
    actions   = setsubtract(each.value.allow_actions, local.managed_bucket_permitted)
    resources = local.managed_bucket_resources
  }

  statement {
    effect    = "Deny"
    actions   = setsubtract(each.value.allow_actions, local.repl_bucket_permitted)
    resources = local.repl_bucket_resources
  }
}

resource "aws_iam_group" "user_groups" {
  for_each = local.user_group_policies

  name = each.value.group_name
  path = "/"
}

resource "aws_iam_policy" "user_groups" {
  for_each = local.user_group_policies

  name        = each.value.policy_name
  description = each.value.description
  policy      = data.aws_iam_policy_document.user_groups[each.key].json
}

resource "aws_iam_group_policy_attachment" "user_groups" {
  for_each = local.user_group_policies

  group      = aws_iam_group.user_groups[each.key].name
  policy_arn = aws_iam_policy.user_groups[each.key].arn
}
