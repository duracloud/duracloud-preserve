# Trail for stack events
resource "aws_cloudtrail" "main" {
  name                          = "${local.stack}-cloudtrail"
  s3_bucket_name                = aws_s3_bucket.main["managed"].id
  s3_key_prefix                 = local.cloudtrail_prefix
  include_global_service_events = false
  is_multi_region_trail         = false

  event_selector {
    read_write_type           = "All"
    include_management_events = true
  }

  depends_on = [aws_s3_bucket_policy.managed]
}
