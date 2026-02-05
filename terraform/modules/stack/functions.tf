locals {
  functions = var.deploy_functions ? var.functions : {}

  architectures = ["arm64"]
  handler       = "bootstrap" # irrelevant for binaries
  package_type  = "Zip"
  runtime       = "provided.al2023"
}

resource "aws_lambda_function" "main" {
  for_each = local.functions

  architectures = local.architectures
  function_name = "${local.stack}-${each.key}"
  handler       = local.handler
  memory_size   = each.value.memory
  timeout       = each.value.timeout
  package_type  = local.package_type
  role          = aws_iam_role.lambda[each.key].arn
  runtime       = local.runtime
  s3_bucket     = each.value.bucket
  s3_key        = each.value.file

  environment {
    variables = {
      STACK = local.stack
    }
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
