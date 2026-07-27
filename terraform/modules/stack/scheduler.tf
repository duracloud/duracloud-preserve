# EventBridge Scheduler resources for scheduled Lambda functions
locals {
  scheduled_functions = merge(local.deploy_storage_report, local.deploy_compute_checksums)

  deploy_scheduler = length(local.scheduled_functions) > 0 ? 1 : 0
}

data "aws_iam_policy_document" "scheduler_assume_role" {
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
  count = local.deploy_scheduler

  name               = "${local.stack}-scheduler"
  assume_role_policy = data.aws_iam_policy_document.scheduler_assume_role.json
}

data "aws_iam_policy_document" "scheduler" {
  statement {
    effect    = "Allow"
    actions   = ["lambda:InvokeFunction"]
    resources = [for k, _ in local.scheduled_functions : aws_lambda_function.main[k].arn]
  }
}

resource "aws_iam_role_policy" "scheduler" {
  count = local.deploy_scheduler

  role   = aws_iam_role.scheduler[0].id
  policy = data.aws_iam_policy_document.scheduler.json
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
    role_arn = aws_iam_role.scheduler[0].arn
  }

  depends_on = [aws_iam_role_policy.scheduler]
}
