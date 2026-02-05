# Bucket request policy, permissions and notifications
resource "aws_iam_role_policy" "bucket_request" {
  count = local.deploy_functions ? 1 : 0

  role = aws_iam_role.lambda["bucket-request"].name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "s3:CreateBucket",
          "s3:DeleteBucket",
          "s3:DeleteBucketPolicy",
          "s3:GetObject",
          "s3:PutBucketAcl",
          "s3:PutBucketLifecycleConfiguration",
          "s3:PutBucketLogging",
          "s3:PutBucketNotificationConfiguration",
          "s3:PutBucketInventoryConfiguration",
          "s3:PutBucketOwnershipControls",
          "s3:PutBucketPolicy",
          "s3:PutBucketPublicAccessBlock",
          "s3:PutBucketReplication",
          "s3:PutBucketTagging",
          "s3:PutBucketVersioning",
          "s3:PutObject"
        ]
        Resource = "arn:aws:s3:::*"
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

resource "aws_iam_role_policy_attachment" "bucket_request" {
  count = local.deploy_functions ? 1 : 0

  policy_arn = local.basic_role
  role       = aws_iam_role.lambda["bucket-request"].name
}

resource "aws_lambda_permission" "bucket_request" {
  count = local.deploy_functions ? 1 : 0

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main["bucket-request"].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["bucket-request"].arn

  depends_on = [aws_lambda_function.main["bucket-request"]]
}

resource "aws_s3_bucket_notification" "bucket_request" {
  count = local.deploy_functions ? 1 : 0

  bucket      = aws_s3_bucket.main["bucket-request"].id
  eventbridge = true

  lambda_function {
    lambda_function_arn = aws_lambda_function.main["bucket-request"].arn
    events              = ["s3:ObjectCreated:*"]
    filter_suffix       = ".txt"
  }

  depends_on = [aws_lambda_permission.bucket_request]
}
