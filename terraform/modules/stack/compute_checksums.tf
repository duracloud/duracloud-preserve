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
        Effect = "Allow"
        Action = [
          "s3:CreateJob",
          "s3:GetAccountPublicAccessBlock",
          "s3:ListAllMyBuckets"
        ]
        Resource = "*"
      },
      {
        Effect = "Allow"
        Action = [
          "s3:GetBucketTagging",
          "s3:ListTagsForResource"
        ]
        Resource = "arn:aws:s3:::${local.stack}-*"
      },
      {
        Effect   = "Allow"
        Action   = "s3:PutObject"
        Resource = "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*"
      },
      {
        Effect   = "Allow"
        Action   = "iam:PassRole"
        Resource = aws_iam_role.batch.arn
        Condition = {
          StringEquals = {
            "iam:PassedToService" = [
              "s3.amazonaws.com",
              "batchoperations.s3.amazonaws.com"
            ]
          }
        }
      }
    ]
  })
}

resource "aws_scheduler_schedule" "compute_checksums" {
  count = local.deploy_compute_checksums ? 1 : 0

  name                         = "${local.stack}-compute-checksums-schedule"
  schedule_expression          = local.functions["compute-checksums"].schedule
  schedule_expression_timezone = local.functions["compute-checksums"].tz

  flexible_time_window {
    mode = "OFF"
  }

  target {
    arn      = aws_lambda_function.main["compute-checksums"].arn
    role_arn = aws_iam_role.compute_checksums_scheduler[0].arn
  }

  depends_on = [aws_iam_role_policy.compute_checksums_scheduler[0]]
}

resource "aws_iam_role" "compute_checksums_scheduler" {
  count = local.deploy_compute_checksums ? 1 : 0

  name = "${local.stack}-compute-checksums-scheduler"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "scheduler.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy" "compute_checksums_scheduler" {
  count = local.deploy_compute_checksums ? 1 : 0

  role = aws_iam_role.compute_checksums_scheduler[0].id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = "lambda:InvokeFunction"
      Resource = aws_lambda_function.main["compute-checksums"].arn
    }]
  })
}
