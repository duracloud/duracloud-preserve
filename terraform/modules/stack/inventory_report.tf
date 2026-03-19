# Inventory report policy, permissions and notifications
locals {
  deploy_inventory_report = contains(keys(local.functions), "inventory-report") ? { "inventory-report" = {} } : {}
}

data "aws_iam_policy_document" "inventory_report" {
  for_each = local.deploy_inventory_report

  statement {
    effect    = "Allow"
    actions   = ["s3:GetObject"]
    resources = ["${aws_s3_bucket.main["managed"].arn}/${local.inventory_prefix}/*"]
  }

  statement {
    effect  = "Allow"
    actions = ["s3:PutObject"]
    resources = [
      "${aws_s3_bucket.main["managed"].arn}/${local.reports_prefix}/*",
      "${aws_s3_bucket.main["managed"].arn}/${local.metadata_prefix}/*",
    ]
  }
}

resource "aws_iam_role_policy" "inventory_report" {
  for_each = local.deploy_inventory_report

  role   = aws_iam_role.lambda[each.key].name
  policy = data.aws_iam_policy_document.inventory_report[each.key].json
}

resource "aws_lambda_permission" "inventory_report" {
  for_each = local.deploy_inventory_report

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main[each.key].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["managed"].arn
}
