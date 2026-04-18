locals {
  memberships = {
    for m in flatten([
      for name, u in local.users : [
        for mem in u.memberships : {
          key   = "${name}_${mem.stack}"
          user  = name
          stack = mem.stack
          group = mem.group
        }
      ]
    ]) : m.key => m
  }

  user_buckets = {
    for name, u in var.users : name => u.buckets
    if length(u.buckets) > 0
  }

  user_managed_buckets = {
    for name, buckets in local.user_buckets : name => [
      for bucket in buckets : bucket
      if endswith(bucket, local.managed_suffix)
    ]
  }

  user_repl_buckets = {
    for name, buckets in local.user_buckets : name => [
      for bucket in buckets : bucket
      if endswith(bucket, local.replication_suffix)
    ]
  }

  user_managed_bucket_resources = {
    for name, buckets in local.user_managed_buckets : name => concat(
      [for bucket in buckets : "arn:aws:s3:::${bucket}"],
      [for bucket in buckets : "arn:aws:s3:::${bucket}/*"]
    )
  }

  user_repl_bucket_resources = {
    for name, buckets in local.user_repl_buckets : name => concat(
      [for bucket in buckets : "arn:aws:s3:::${bucket}"],
      [for bucket in buckets : "arn:aws:s3:::${bucket}/*"]
    )
  }

  user_bucket_allow_actions = [
    "s3:ListBucket",
    "s3:ListBucketMultipartUploads",
  ]

  user_object_allow_actions = [
    "s3:GetObject",
    "s3:PutObject",
    "s3:AbortMultipartUpload",
    "s3:ListMultipartUploadParts",
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

data "aws_iam_group" "user" {
  for_each = local.memberships

  group_name = "${each.value.stack}-${each.value.group}"
}

resource "aws_iam_user" "user" {
  for_each = local.users

  name = each.key

  tags = {
    Email = each.value.email
  }
}

resource "aws_iam_access_key" "user" {
  for_each = local.users

  user = aws_iam_user.user[each.key].name
}

resource "aws_iam_user_group_membership" "user" {
  for_each = local.memberships

  user   = aws_iam_user.user[each.value.user].name
  groups = [data.aws_iam_group.user[each.key].group_name]
}

resource "aws_ssm_parameter" "access_key" {
  for_each = local.users

  name        = "${local.user_access_key_namespace}${each.key}"
  description = "Access key for IAM user ${each.key}"
  type        = "String"
  value       = aws_iam_access_key.user[each.key].id
}

resource "aws_ssm_parameter" "secret_key" {
  for_each = local.users

  name        = "${local.user_secret_key_namespace}${each.key}"
  description = "Secret key for IAM user ${each.key}"
  type        = "SecureString"
  value       = aws_iam_access_key.user[each.key].secret
}

data "aws_iam_policy_document" "s3_access" {
  for_each = local.user_buckets

  statement {
    effect    = "Allow"
    actions   = local.user_bucket_allow_actions
    resources = [for bucket in each.value : "arn:aws:s3:::${bucket}"]
  }

  statement {
    effect    = "Allow"
    actions   = local.user_object_allow_actions
    resources = [for bucket in each.value : "arn:aws:s3:::${bucket}/*"]
  }

  dynamic "statement" {
    for_each = length(local.user_managed_bucket_resources[each.key]) > 0 ? [local.user_managed_bucket_resources[each.key]] : []

    content {
      effect    = "Deny"
      actions   = local.managed_bucket_deny_actions
      resources = statement.value
    }
  }

  dynamic "statement" {
    for_each = length(local.user_repl_bucket_resources[each.key]) > 0 ? [local.user_repl_bucket_resources[each.key]] : []

    content {
      effect    = "Deny"
      actions   = local.repl_bucket_deny_actions
      resources = statement.value
    }
  }
}

resource "aws_iam_user_policy" "s3_access" {
  for_each = local.user_buckets

  name = "s3-access"
  user = aws_iam_user.user[each.key].name

  policy = data.aws_iam_policy_document.s3_access[each.key].json
}
