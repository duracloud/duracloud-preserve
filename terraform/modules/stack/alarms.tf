locals {
  billing_threshold    = var.billing_alert_threshold
  emails_to_notify     = var.emails_to_notify
  email_alarms_enabled = length(local.emails_to_notify) > 0
}

resource "aws_sns_topic" "email_notification" {
  count = local.email_alarms_enabled ? 1 : 0

  name = "${local.stack}-email-notification"
}

resource "aws_sns_topic_subscription" "email_notification" {
  for_each = toset(local.emails_to_notify)

  topic_arn = aws_sns_topic.email_notification[0].arn
  protocol  = "email"
  endpoint  = each.key
}

// Billing alarms are per account so in the event that multiple stacks are
// deployed to a single account the billing threshold should be set high
// enough to cover all stacks
resource "aws_cloudwatch_metric_alarm" "billing_alarm" {
  count = local.billing_threshold != null ? 1 : 0

  alarm_name          = "${local.stack}-billing-alarm"
  alarm_description   = "Billing threshold exceeded: > ${local.billing_threshold}"
  comparison_operator = "GreaterThanOrEqualToThreshold"
  evaluation_periods  = "1"
  metric_name         = "EstimatedCharges"
  namespace           = "AWS/Billing"
  period              = "86400"
  statistic           = "Maximum"
  threshold           = local.billing_threshold
  treat_missing_data  = "notBreaching"

  dimensions = {
    Currency      = "USD"
    LinkedAccount = local.account_id
  }

  alarm_actions = local.email_alarms_enabled ? [aws_sns_topic.email_notification[0].arn] : []
}

resource "aws_cloudwatch_metric_alarm" "function" {
  for_each = local.functions

  alarm_name          = "${local.stack}-${each.key}-error-alarm"
  alarm_description   = "Error encountered while processing function"
  comparison_operator = "GreaterThanOrEqualToThreshold"
  evaluation_periods  = "1"
  metric_name         = "Errors"
  namespace           = "AWS/Lambda"
  period              = "300"
  statistic           = "Sum"
  threshold           = "1"
  treat_missing_data  = "notBreaching"

  dimensions = {
    FunctionName = aws_lambda_function.main[each.key].function_name
  }

  alarm_actions = local.email_alarms_enabled ? [aws_sns_topic.email_notification[0].arn] : []
}
