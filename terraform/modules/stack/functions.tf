locals {
  deploy_functions = var.deploy_functions

  functions = var.deploy_functions ? {
    bucket-request = { memory_size = 128 }
    # checksum-report   = {}
    # compute-checksums = {}
    # inventory-report  = {}
    # storage-report    = {}
  } : {}

  function_bucket = var.function_bucket
  function_files  = var.function_files
  function_prefix = var.function_prefix

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
  memory_size   = each.value.memory_size
  package_type  = local.package_type
  role          = aws_iam_role.lambda[each.key].arn
  runtime       = local.runtime
  s3_bucket     = local.function_bucket
  s3_key        = "${local.function_prefix}/${local.function_files[each.key]}"

  logging_config {
    log_format = "JSON"
    log_group  = aws_cloudwatch_log_group.main[each.key]
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
