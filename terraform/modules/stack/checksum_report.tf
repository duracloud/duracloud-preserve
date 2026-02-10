# Checksum report policy, permissions and notifications

# resource "aws_cloudwatch_event_target" "checksum_report" {
#   rule      = aws_cloudwatch_event_rule.batch_job_complete.name
#   target_id = "checksum-report"
#   arn       = aws_lambda_function.main["checksum-report"].arn
# }
