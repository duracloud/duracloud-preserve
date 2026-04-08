locals {
  bucket_notifications = {
    managed = {
      eventbridge = false
      entries = concat(
        # Inventory report: fires on manifest.json written to the manifests prefix
        [
          for k, _ in local.deploy_inventory_report : {
            id            = "inventory-report-manifest"
            lambda_arn    = aws_lambda_function.main[k].arn
            events        = ["s3:ObjectCreated:*"]
            filter_prefix = "${local.manifests_prefix}/"
            filter_suffix = "manifest.json"
          }
        ],
      )
    }
    request = {
      eventbridge = true
      entries = concat(
        # Bucket request: fires on .txt written to the bucket request prefix
        [
          for k, _ in local.deploy_bucket_request : {
            id            = "bucket-request-trigger"
            lambda_arn    = aws_lambda_function.main[k].arn
            events        = ["s3:ObjectCreated:*"]
            filter_prefix = local.bucket_request_prefix
            filter_suffix = ".txt"
          }
        ],
      )
    }
  }
}

resource "aws_s3_bucket_notification" "main" {
  for_each = { for k, v in local.bucket_notifications : k => v if length(v.entries) > 0 || v.eventbridge }

  bucket      = aws_s3_bucket.main[each.key].id
  eventbridge = each.value.eventbridge

  dynamic "lambda_function" {
    for_each = { for n in each.value.entries : n.id => n }
    content {
      id                  = lambda_function.key
      lambda_function_arn = lambda_function.value.lambda_arn
      events              = lambda_function.value.events
      filter_prefix       = lambda_function.value.filter_prefix
      filter_suffix       = lambda_function.value.filter_suffix
    }
  }

  depends_on = [
    aws_lambda_permission.bucket_request,
    aws_lambda_permission.inventory_report,
  ]
}
