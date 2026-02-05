# Inventory report policy, permissions and notifications
locals {
  deploy_inventory_report = contains(keys(local.functions), "inventory-report")
}

resource "aws_lambda_permission" "inventory_report" {
  count = local.deploy_inventory_report ? 1 : 0

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main["inventory-report"].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["managed"].arn

  depends_on = [aws_lambda_function.main["inventory_report"]]
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
