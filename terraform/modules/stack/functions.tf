locals {
  functions = var.deploy_functions ? var.functions : {}

  architectures = ["arm64"]
  handler       = "bootstrap" # irrelevant for binaries
  package_type  = "Zip"
  runtime       = "provided.al2023"
}

data "aws_s3_object" "main" {
  for_each = local.functions

  bucket = each.value.bucket
  key    = each.value.file
}

resource "aws_lambda_function" "main" {
  for_each = local.functions

  architectures    = local.architectures
  function_name    = "${local.stack}-${each.key}"
  handler          = local.handler
  memory_size      = each.value.memory
  timeout          = each.value.timeout
  package_type     = local.package_type
  role             = aws_iam_role.lambda[each.key].arn
  runtime          = local.runtime
  s3_bucket        = data.aws_s3_object.main[each.key].bucket
  s3_key           = data.aws_s3_object.main[each.key].key
  source_code_hash = data.aws_s3_object.main[each.key].etag

  environment {
    variables = merge(
      { STACK = local.stack },
      each.value.env
    )
  }

  logging_config {
    log_format = "JSON"
    log_group  = aws_cloudwatch_log_group.main[each.key].name
  }
}

resource "aws_cloudwatch_log_group" "main" {
  for_each = local.functions

  name              = "/aws/lambda/${local.stack}-${each.key}"
  retention_in_days = 7
}

# resource "aws_cloudwatch_metric_alarm" "main" {
#   for_each = local.functions

#   alarm_name          = "${local.stack}-${each.key}-alarm"
#   comparison_operator = "GreaterThanThreshold"
#   evaluation_periods  = "1"
#   metric_name         = "Errors"
#   namespace           = "AWS/Lambda"
#   period              = "300"
#   statistic           = "Sum"
#   threshold           = "0"
#   alarm_description   = "Error processing ${local.stack} ${each.key}"
#   treat_missing_data  = "notBreaching"

#   dimensions = {
#     FunctionName = aws_lambda_function.main[each.key].function_name
#   }

#   alarm_actions = []
# }

resource "aws_iam_role" "lambda" {
  for_each = local.functions

  name = "${local.stack}-${each.key}"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action    = "sts:AssumeRole"
      Effect    = "Allow"
      Principal = { Service = "lambda.amazonaws.com" }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "lambda" {
  for_each = local.functions

  policy_arn = local.basic_role
  role       = aws_iam_role.lambda[each.key].name
}

resource "aws_iam_role_policy" "role_access" {
  for_each = local.functions

  role = aws_iam_role.lambda[each.key].name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = "iam:GetRole"
        Resource = [
          aws_iam_role.batch.arn,
          aws_iam_role.replication.arn,
        ]
      }
    ]
  })
}
