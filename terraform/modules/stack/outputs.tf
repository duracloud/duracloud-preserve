
output "batch_job_complete_rule_name" {
  description = "Name of the EventBridge rule for batch job completion"
  value       = aws_cloudwatch_event_rule.batch_job_complete.name
}

output "batch_role_arn" {
  description = "ARN of the S3 Batch Operations role"
  value       = aws_iam_role.batch.arn
}

output "managed_bucket_name" {
  description = "Name of the managed bucket"
  value       = aws_s3_bucket.main["managed"].id
}

output "replication_role_arn" {
  description = "ARN of the S3 replication role"
  value       = aws_iam_role.replication.arn
}

output "request_bucket_name" {
  description = "Name of the request bucket"
  value       = aws_s3_bucket.main["bucket-request"].id
}
