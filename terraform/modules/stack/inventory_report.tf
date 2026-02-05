# Inventory report policy, permissions and notifications
resource "aws_lambda_permission" "inventory_report" {
  count = local.deploy_functions ? 1 : 0

  statement_id  = "AllowExecutionFromS3"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.main["inventory-report"].function_name
  principal     = "s3.amazonaws.com"
  source_arn    = aws_s3_bucket.main["managed"].arn

  depends_on = [aws_lambda_function.main["inventory_report"]]
}
