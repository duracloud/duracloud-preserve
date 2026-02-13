# Checksum report policy, permissions and notifications
locals {
  deploy_checksum_report = contains(keys(local.functions), "checksum-report")
}

resource "aws_iam_role_policy" "checksum_report" {
  count = local.deploy_checksum_report ? 1 : 0

  role = aws_iam_role.lambda["checksum-report"].name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "s3:GetObject",
        ]
        Resource = [
          "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*",
          "${aws_s3_bucket.main["managed"].arn}/${local.batch_prefix}/reports/checksum/*",
          "${aws_s3_bucket.main["managed"].arn}/${local.batch_prefix}/manifests/checksum/*",
        ]
      },
      {
        Effect = "Allow"
        Action = [
          "s3:PutObject",
        ]
        Resource = [
          "${aws_s3_bucket.main["managed"].arn}/${local.reports_prefix}/*",
          "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*",
        ]
      },
      {
        Effect = "Allow"
        Action = [
          "s3:DescribeJob",
        ]
        Resource = "*"
      }
    ]
  })
}

resource "aws_cloudwatch_event_target" "checksum_report" {
  count = local.deploy_checksum_report ? 1 : 0

  rule      = aws_cloudwatch_event_rule.batch_job_complete.name
  target_id = "checksum-report"
  arn       = aws_lambda_function.main["checksum-report"].arn

  depends_on = [aws_lambda_permission.checksum_report]
}

resource "aws_lambda_permission" "checksum_report" {
  count = local.deploy_checksum_report ? 1 : 0

  statement_id  = "AllowExecutionFromEventBridge"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main["checksum-report"].function_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.batch_job_complete.arn

  depends_on = [aws_lambda_function.main["checksum-report"]]
}
