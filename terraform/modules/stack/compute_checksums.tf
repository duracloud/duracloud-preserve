# Compute checksums policy, permissions and schedule
locals {
  deploy_compute_checksums = contains(keys(local.functions), "compute-checksums") ? { "compute-checksums" = {} } : {}
}

data "aws_iam_policy_document" "compute_checksums" {
  for_each = local.deploy_compute_checksums

  statement {
    effect = "Allow"
    actions = [
      "s3:CreateJob",
      "s3:GetAccountPublicAccessBlock",
      "s3:ListAllMyBuckets",
    ]
    resources = ["*"]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:GetBucketTagging",
      "s3:ListTagsForResource",
    ]
    resources = [local.stack_bucket_arn_pattern]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:PutObject"]
    resources = ["${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*"]
  }

  statement {
    effect    = "Allow"
    actions   = ["iam:PassRole"]
    resources = [aws_iam_role.batch.arn]

    condition {
      test     = "StringEquals"
      variable = "iam:PassedToService"
      values = [
        "s3.amazonaws.com",
        "batchoperations.s3.amazonaws.com",
      ]
    }
  }
}

resource "aws_iam_role_policy" "compute_checksums" {
  for_each = local.deploy_compute_checksums

  role   = aws_iam_role.lambda[each.key].name
  policy = data.aws_iam_policy_document.compute_checksums[each.key].json
}

data "aws_iam_policy_document" "compute_checksums_scheduler_assume_role" {
  for_each = local.deploy_compute_checksums

  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["scheduler.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "compute_checksums_scheduler" {
  for_each = local.deploy_compute_checksums

  name               = "${local.stack}-compute-checksums-scheduler"
  assume_role_policy = data.aws_iam_policy_document.compute_checksums_scheduler_assume_role[each.key].json
}

data "aws_iam_policy_document" "compute_checksums_scheduler" {
  for_each = local.deploy_compute_checksums

  statement {
    effect    = "Allow"
    actions   = ["lambda:InvokeFunction"]
    resources = [aws_lambda_function.main[each.key].arn]
  }
}

resource "aws_iam_role_policy" "compute_checksums_scheduler" {
  for_each = local.deploy_compute_checksums

  role   = aws_iam_role.compute_checksums_scheduler[each.key].id
  policy = data.aws_iam_policy_document.compute_checksums_scheduler[each.key].json
}

resource "aws_scheduler_schedule" "compute_checksums" {
  for_each = local.deploy_compute_checksums

  name                         = "${local.stack}-compute-checksums-schedule"
  schedule_expression          = local.functions[each.key].schedule
  schedule_expression_timezone = local.functions[each.key].tz

  flexible_time_window {
    mode = "OFF"
  }

  target {
    arn      = aws_lambda_function.main[each.key].arn
    role_arn = aws_iam_role.compute_checksums_scheduler[each.key].arn
  }

  depends_on = [aws_iam_role_policy.compute_checksums_scheduler]
}
