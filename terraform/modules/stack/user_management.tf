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
      managed_bucket_deny_actions = [
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts",
        "s3:ListBucketMultipartUploads",
      ]
      repl_bucket_deny_actions = [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts",
        "s3:ListBucketMultipartUploads",
      ]
    }
    restricted_users = {
      group_name  = "${local.stack}-restricted-users"
      policy_name = "${local.stack}-restricted-users-policy"
      description = "Policy for restricted users"
      # No bucket/object access by default, addtl permissions must come from user policy
      allow_actions               = []
      managed_bucket_deny_actions = []
      repl_bucket_deny_actions    = []
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
      managed_bucket_deny_actions = [
        "s3:PutObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts",
        "s3:ListBucketMultipartUploads",
      ]
      repl_bucket_deny_actions = [
        "s3:GetObject",
        "s3:PutObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts",
        "s3:ListBucketMultipartUploads",
      ]
    }
  }

  # Scoped stack buckets (including request)
  stack_bucket_resources = [
    local.stack_bucket_arn_pattern,
    local.stack_object_arn_pattern,
  ]

  managed_bucket_resources = [
    local.managed_bucket_arn,
    local.managed_bucket_object_arn,
  ]

  repl_bucket_resources = [
    local.repl_bucket_arn_pattern,
    local.repl_object_arn_pattern,
  ]
}

data "aws_iam_policy_document" "user_groups" {
  for_each = local.user_group_policies

  statement {
    effect    = "Allow"
    actions   = ["s3:ListAllMyBuckets"]
    resources = ["*"]
  }

  dynamic "statement" {
    for_each = toset(each.value.allow_actions)

    content {
      effect    = "Allow"
      actions   = [statement.value]
      resources = local.stack_bucket_resources
    }
  }

  dynamic "statement" {
    for_each = toset(each.value.managed_bucket_deny_actions)

    content {
      effect    = "Deny"
      actions   = [statement.value]
      resources = local.managed_bucket_resources
    }
  }

  dynamic "statement" {
    for_each = toset(each.value.repl_bucket_deny_actions)

    content {
      effect    = "Deny"
      actions   = [statement.value]
      resources = local.repl_bucket_resources
    }
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
