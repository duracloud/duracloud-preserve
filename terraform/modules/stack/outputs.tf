
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
  value       = aws_s3_bucket.main["request"].id
}

output "cloudfront_domain_name" {
  value = try(aws_cloudfront_distribution.public["public"].domain_name, null)
}

output "cloudfront_hosted_zone_id" {
  value = try(aws_cloudfront_distribution.public["public"].hosted_zone_id, null)
}

output "cloudfront_signed_base_url" {
  description = "Base URL for signed URL access (null when signed access is disabled)"
  value = local.signed_enabled ? "https://${
    local.custom_domain ? local.fqdn : aws_cloudfront_distribution.public["public"].domain_name
  }${local.signed_path}" : null
}

output "cloudfront_signed_key_ids" {
  description = "CloudFront public key IDs (Key-Pair-Id) by signer name"
  value       = { for name, key in aws_cloudfront_public_key.signed : name => key.id }
}
