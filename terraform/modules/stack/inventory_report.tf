# Inventory report policy, permissions and notifications
locals {
  deploy_inventory_report = contains(keys(local.functions), "inventory-report")
}

resource "aws_iam_role_policy" "inventory_report" {
  count = local.deploy_inventory_report ? 1 : 0

  role = aws_iam_role.lambda["inventory-report"].name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["s3:GetObject"]
        Resource = "${aws_s3_bucket.main["managed"].arn}/${local.inventory_prefix}/*"
      },
      {
        Effect = "Allow"
        Action = ["s3:PutObject"]
        Resource = [
          "${aws_s3_bucket.main["managed"].arn}/${local.reports_prefix}/*",
          "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*"
        ]
      }
    ]
  })
}

resource "aws_lambda_permission" "inventory_report" {
  count = local.deploy_inventory_report ? 1 : 0

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main["inventory-report"].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["managed"].arn

  depends_on = [aws_lambda_function.main["inventory-report"]]
}

# TODO: this works while inventory report is the only receiver
# but this may have to be generalized at some point in the future
resource "aws_s3_bucket_notification" "managed" {
  count = local.deploy_inventory_report ? 1 : 0

  bucket = aws_s3_bucket.main["managed"].id

  lambda_function {
    lambda_function_arn = aws_lambda_function.main["inventory-report"].arn
    events              = ["s3:ObjectCreated:*"]
    filter_prefix       = "${local.inventory_prefix}/"
    filter_suffix       = "manifest.json"
  }

  depends_on = [
    aws_lambda_permission.inventory_report
  ]
}
