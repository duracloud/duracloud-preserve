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
#   target_id = "checksum-report"
#   arn       = aws_lambda_function.main["compute-checksums"].arn
# }
