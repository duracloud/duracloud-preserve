# Checksum report policy, permissions and notifications
locals {
  deploy_checksum_report = contains(keys(local.functions), "checksum-report") ? { "checksum-report" = {} } : {}
}

data "aws_iam_policy_document" "checksum_report" {
  for_each = local.deploy_checksum_report

  statement {
    effect = "Allow"
    actions = [
      "s3:GetObject",
    ]
    resources = [
      "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*",
      "${aws_s3_bucket.main["managed"].arn}/${local.batch_prefix}/reports/checksum/*",
      "${aws_s3_bucket.main["managed"].arn}/${local.batch_prefix}/manifests/checksum/*",
    ]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:PutObject",
    ]
    resources = [
      "${aws_s3_bucket.main["managed"].arn}/${local.reports_prefix}/*",
      "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*",
    ]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:DescribeJob"]
    resources = ["*"]
  }
}

resource "aws_iam_role_policy" "checksum_report" {
  for_each = local.deploy_checksum_report

  role   = aws_iam_role.lambda[each.key].name
  policy = data.aws_iam_policy_document.checksum_report[each.key].json
}

resource "aws_cloudwatch_event_target" "checksum_report" {
  for_each = local.deploy_checksum_report

  rule      = aws_cloudwatch_event_rule.batch_job_complete.name
  target_id = each.key
  arn       = aws_lambda_function.main[each.key].arn

  depends_on = [aws_lambda_permission.checksum_report]
}

resource "aws_lambda_permission" "checksum_report" {
  for_each = local.deploy_checksum_report

  statement_id  = "AllowExecutionFromEventBridge"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main[each.key].function_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.batch_job_complete.arn
}
