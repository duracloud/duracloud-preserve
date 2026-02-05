# Event rule for S3 Batch Operations job completion
resource "aws_cloudwatch_event_rule" "batch_job_complete" {
  name        = "${local.stack}-batch-job-complete"
  description = "Triggers on S3 Batch Operations job completion or failure"

  event_pattern = jsonencode({
    source      = ["aws.s3"]
    detail-type = ["AWS Service Event via CloudTrail"]
    detail = {
      eventSource = ["s3.amazonaws.com"]
      eventName   = ["JobStatusChanged"]
      serviceEventDetails = {
        status = ["Complete", "Failed"]
      }
    }
  })
}

# Event target associating batch job complete rule with lambda functions
# resource "aws_cloudwatch_event_target" "batch_job_complete_for_compute_checksums" {
#   rule      = aws_cloudwatch_event_rule.batch_job_complete.name
#   target_id = "compute_checksums"
#   arn       = aws_lambda_function.main["compute-checksums"].arn
# }

# Managed bucket notifications
# resource "aws_s3_bucket_notification" "managed" {
#   bucket = aws_s3_bucket.main["managed"].id

#   lambda_function {
#     lambda_function_arn = aws_lambda_function.main["inventory-report"].arn
#     events              = ["s3:ObjectCreated:*"]
#     filter_prefix       = "${local.inventory_prefix}/"
#     filter_suffix       = "manifest.json"
#   }

#   depends_on = [
#     aws_lambda_permission.inventory_report
#   ]
# }
