# Inventory report policy, permissions and notifications
locals {
  deploy_compute_checksums = contains(keys(local.functions), "compute-checksums")
}

resource "aws_iam_role_policy" "compute_checksums" {
  count = local.deploy_compute_checksums ? 1 : 0

  role = aws_iam_role.lambda["compute-checksums"].name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["s3:GetObject"]
        Resource = "${aws_s3_bucket.main["managed"].arn}/${local.batch_prefix}/*"
      },
      {
        Effect   = "Allow"
        Action   = ["s3:PutObject"]
        Resource = "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*"
      },
      {
        Effect   = "Allow"
        Action   = "iam:PassRole"
        Resource = aws_iam_role.batch.arn
        Condition = {
          StringEquals = {
            "iam:PassedToService" = "batchoperations.s3.amazonaws.com"
          }
        }
      }
    ]
  })
}

resource "aws_lambda_permission" "compute_checksums" {
  count = local.deploy_compute_checksums ? 1 : 0

  statement_id  = "AllowExecutionFromEventBridge"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main["compute-checksums"].function_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.compute_checksums[0].arn

  depends_on = [aws_lambda_function.main["compute-checksums"]]
}

resource "aws_cloudwatch_event_rule" "compute_checksums" {
  count = local.deploy_compute_checksums ? 1 : 0

  name                = "${local.stack}-compute-checksums-schedule"
  description         = "Trigger compute checksums job"
  schedule_expression = local.functions["compute-checksums"].schedule
  state               = "ENABLED"
}

resource "aws_cloudwatch_event_target" "compute_checksums" {
  count = local.deploy_compute_checksums ? 1 : 0

  rule      = aws_cloudwatch_event_rule.compute_checksums[0].name
  target_id = "compute-checksums"
  arn       = aws_lambda_function.main["compute-checksums"].arn
}
