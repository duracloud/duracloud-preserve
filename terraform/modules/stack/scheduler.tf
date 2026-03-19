# EventBridge Scheduler resources for scheduled Lambda functions
locals {
  scheduled_functions = merge(local.deploy_storage_report, local.deploy_compute_checksums)
}

data "aws_iam_policy_document" "scheduler_assume_role" {
  for_each = local.scheduled_functions

  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["scheduler.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "scheduler" {
  for_each = local.scheduled_functions

  name               = "${local.stack}-${each.key}-scheduler"
  assume_role_policy = data.aws_iam_policy_document.scheduler_assume_role[each.key].json
}

data "aws_iam_policy_document" "scheduler" {
  for_each = local.scheduled_functions

  statement {
    effect    = "Allow"
    actions   = ["lambda:InvokeFunction"]
    resources = [aws_lambda_function.main[each.key].arn]
  }
}

resource "aws_iam_role_policy" "scheduler" {
  for_each = local.scheduled_functions

  role   = aws_iam_role.scheduler[each.key].id
  policy = data.aws_iam_policy_document.scheduler[each.key].json
}

resource "aws_scheduler_schedule" "main" {
  for_each = local.scheduled_functions

  name                         = "${local.stack}-${each.key}-schedule"
  schedule_expression          = local.functions[each.key].schedule
  schedule_expression_timezone = local.functions[each.key].tz

  flexible_time_window {
    mode = "OFF"
  }

  target {
    arn      = aws_lambda_function.main[each.key].arn
    role_arn = aws_iam_role.scheduler[each.key].arn
  }

  depends_on = [aws_iam_role_policy.scheduler]
}
