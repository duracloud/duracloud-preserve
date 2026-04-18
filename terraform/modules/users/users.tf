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

  name        = "/iam/access_key/${each.key}"
  description = "Access key for IAM user ${each.key}"
  type        = "String"
  value       = aws_iam_access_key.user[each.key].id
}

resource "aws_ssm_parameter" "secret_key" {
  for_each = local.users

  name        = "/iam/secret_key/${each.key}"
  description = "Secret key for IAM user ${each.key}"
  type        = "SecureString"
  value       = aws_iam_access_key.user[each.key].secret
}

resource "aws_iam_user_policy" "s3_access" {
  for_each = local.user_buckets

  name = "s3-access"
  user = aws_iam_user.user[each.key].name

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = [
        "s3:GetObject",
        "s3:PutObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts",
        "s3:ListBucketMultipartUploads",
      ]
      Resource = [for b in each.value : "arn:aws:s3:::${b}/*"]
    }]
  })
}
