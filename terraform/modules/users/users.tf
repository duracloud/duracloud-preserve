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

  # Delete access is opt-in per restricted membership and limited to assigned
  # buckets in that stack. The group-level Deny on managed/repl buckets
  # (terraform/modules/stack/user_management.tf) still protects reserved data.
  user_delete_object_resources = {
    for name, buckets in local.user_buckets : name => [
      for bucket in buckets : "arn:aws:s3:::${bucket}/*"
      if anytrue([
        for m in var.users[name].memberships :
        m.group == "restricted-users" && m.allow_delete && startswith(bucket, "${m.stack}-")
      ])
    ]
  }

  user_bucket_allow_actions = [
    // TODO: remove GetBucketLocation (ZD: 28287)
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetBucketLocation.html
    "s3:GetBucketLocation",
    "s3:ListBucket",
    "s3:ListBucketMultipartUploads",
  ]

  user_object_allow_actions = [
    "s3:GetObject",
    "s3:PutObject",
    "s3:AbortMultipartUpload",
    "s3:ListMultipartUploadParts",
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
    for_each = length(local.user_delete_object_resources[each.key]) > 0 ? [local.user_delete_object_resources[each.key]] : []

    content {
      sid       = "RestrictedBucketDeletes"
      effect    = "Allow"
      actions   = ["s3:DeleteObject"]
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
