# Checksum request policy and permissions
locals {
  deploy_checksum_request = contains(keys(local.functions), "checksum-request") ? { "checksum-request" = {} } : {}
}

data "aws_iam_policy_document" "checksum_request" {
  for_each = local.deploy_checksum_request

  statement {
    effect    = "Allow"
    actions   = ["s3:DeleteObject"]
    resources = ["${aws_s3_bucket.main["request"].arn}/${local.checksum_request_prefix}/*"]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:GetObject"]
    resources = ["${aws_s3_bucket.main["managed"].arn}/${local.reports_prefix}/*/manifests/*"]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:PutObject"]
    resources = ["${aws_s3_bucket.main["managed"].arn}/${local.reports_prefix}/*/checksums/*"]
  }

  statement {
    effect    = "Allow"
    actions   = ["s3:GetObject"]
    resources = [local.stack_object_arn_pattern]
  }
}

resource "aws_iam_role_policy" "checksum_request" {
  for_each = local.deploy_checksum_request

  role   = aws_iam_role.lambda[each.key].name
  policy = data.aws_iam_policy_document.checksum_request[each.key].json
}

resource "aws_lambda_permission" "checksum_request" {
  for_each = local.deploy_checksum_request

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main[each.key].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["request"].arn
}
