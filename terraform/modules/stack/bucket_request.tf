# Bucket request policy, permissions and notifications
locals {
  deploy_bucket_request = contains(keys(local.functions), "bucket-request")
}

resource "aws_iam_role_policy" "bucket_request" {
  count = local.deploy_bucket_request ? 1 : 0

  role = aws_iam_role.lambda["bucket-request"].name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "s3:*"
        Resource = "arn:aws:s3:::${local.stack}-*"
      },
      {
        Effect   = "Allow"
        Action   = "iam:GetRole"
        Resource = aws_iam_role.replication.arn
      },
      {
        Effect   = "Allow"
        Action   = "iam:PassRole"
        Resource = aws_iam_role.replication.arn
        Condition = {
          StringEquals = {
            "iam:PassedToService" = "s3.amazonaws.com"
          }
        }
      }
    ]
  })
}

resource "aws_lambda_permission" "bucket_request" {
  count = local.deploy_bucket_request ? 1 : 0

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main["bucket-request"].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["bucket-request"].arn

  depends_on = [aws_lambda_function.main["bucket-request"]]
}

resource "aws_s3_bucket_notification" "bucket_request" {
  count = local.deploy_bucket_request ? 1 : 0

  bucket      = aws_s3_bucket.main["bucket-request"].id
  eventbridge = true

  lambda_function {
    lambda_function_arn = aws_lambda_function.main["bucket-request"].arn
    events              = ["s3:ObjectCreated:*"]
    filter_suffix       = ".txt"
  }

  depends_on = [aws_lambda_permission.bucket_request]
}
