# Storage report policy, permissions and notifications
locals {
  deploy_storage_report = contains(keys(local.functions), "storage-report") ? { "storage-report" = {} } : {}
}

data "aws_iam_policy_document" "storage_report" {
  for_each = local.deploy_storage_report

  statement {
    effect = "Allow"
    actions = [
      "s3:ListAllMyBuckets",
      "s3:GetBucketTagging",
    ]
    resources = ["*"]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:GetObject"]
    resources = ["${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*"]
  }

  statement {
    effect  = "Allow"
    actions = ["s3:PutObject"]
    resources = [
      "${aws_s3_bucket.main["managed"].arn}/${local.reports_prefix}/*",
      "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*",
    ]
  }
}

resource "aws_iam_role_policy" "storage_report" {
  for_each = local.deploy_storage_report

  role   = aws_iam_role.lambda[each.key].name
  policy = data.aws_iam_policy_document.storage_report[each.key].json
}

resource "aws_ssm_parameter" "storage_capacity" {
  name  = local.storage_capacity_param_name
  type  = "String"
  value = local.storage_capacity
}
