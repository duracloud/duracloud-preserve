locals {
  functions = var.deploy_functions ? var.functions : {}

  deploy_function_infra = length(local.functions) > 0 ? 1 : 0

  architectures = ["arm64"]
  handler       = "bootstrap" # irrelevant for binaries
  package_type  = "Zip"
  runtime       = "provided.al2023"

  function_policy_json = merge(
    { for k, v in data.aws_iam_policy_document.bucket_request : k => v.json },
    { for k, v in data.aws_iam_policy_document.checksum_report : k => v.json },
    { for k, v in data.aws_iam_policy_document.checksum_request : k => v.json },
    { for k, v in data.aws_iam_policy_document.compute_checksums : k => v.json },
    { for k, v in data.aws_iam_policy_document.inventory_report : k => v.json },
    { for k, v in data.aws_iam_policy_document.storage_report : k => v.json },
    { for k, v in data.aws_iam_policy_document.sync_users : k => v.json },
  )
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
  role             = aws_iam_role.lambda[0].arn
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

  ephemeral_storage {
    size = each.value.storage
  }

  logging_config {
    log_format = "JSON"
    log_group  = aws_cloudwatch_log_group.main[0].name
  }

  depends_on = [aws_iam_role_policy.function]
}

resource "aws_cloudwatch_log_group" "main" {
  count = local.deploy_function_infra

  name              = "/dcp/${local.stack}/lambda"
  retention_in_days = 7
}

data "aws_iam_policy_document" "lambda_assume_role" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "lambda" {
  count = local.deploy_function_infra

  name               = "${local.stack}-functions"
  assume_role_policy = data.aws_iam_policy_document.lambda_assume_role.json
}

data "aws_iam_policy_document" "config_access" {
  statement {
    effect    = "Allow"
    actions   = ["account:GetAccountInformation"]
    resources = ["*"]
  }

  statement {
    effect    = "Allow"
    actions   = ["iam:GetRole"]
    resources = [aws_iam_role.batch.arn, aws_iam_role.replication.arn]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:ListBucket"]
    resources = [aws_s3_bucket.main["managed"].arn]
  }

  statement {
    effect    = "Allow"
    actions   = ["ssm:GetParameter"]
    resources = [aws_ssm_parameter.storage_capacity.arn]
  }
}

data "aws_iam_policy_document" "lambda_logs" {
  count = local.deploy_function_infra

  statement {
    effect = "Allow"
    actions = [
      "logs:CreateLogStream",
      "logs:PutLogEvents",
    ]
    resources = ["${aws_cloudwatch_log_group.main[0].arn}:*"]
  }
}

data "aws_iam_policy_document" "function" {
  count = local.deploy_function_infra

  source_policy_documents = concat(
    [
      data.aws_iam_policy_document.config_access.json,
      data.aws_iam_policy_document.lambda_logs[0].json,
    ],
    values(local.function_policy_json),
  )
}

resource "aws_iam_role_policy" "function" {
  count = local.deploy_function_infra

  name   = "${local.stack}-functions"
  role   = aws_iam_role.lambda[0].name
  policy = data.aws_iam_policy_document.function[0].json
}
