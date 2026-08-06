# Core resources for scheduled ECS Fargate tasks
locals {
  cluster_name = "${local.stack}-tasks"

  # Union of all secret ARNs across tasks, for the shared execution role.
  task_secret_arns = distinct(flatten([
    for t in var.tasks : [for s in t.secrets : s.valueFrom]
  ]))

  deploy_tasks = length(var.tasks) > 0 ? 1 : 0
}

data "aws_vpc" "default" {
  count = local.deploy_tasks

  default = true
}

data "aws_subnets" "default" {
  count = local.deploy_tasks

  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.default[0].id]
  }
}

data "aws_security_group" "default" {
  count = local.deploy_tasks

  vpc_id = data.aws_vpc.default[0].id
  name   = "default"
}

resource "aws_ecs_cluster" "this" {
  count = local.deploy_tasks

  name = local.cluster_name
}

# Fargate uses this to pull images and write logs. Shared by all tasks.
resource "aws_iam_role" "task_execution" {
  count = local.deploy_tasks

  name = "${local.cluster_name}-execution"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ecs-tasks.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy" "task_execution" {
  count = local.deploy_tasks

  name = "${local.cluster_name}-execution"
  role = aws_iam_role.task_execution[0].id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = concat(
      [{
        Effect = "Allow"
        Action = [
          "ecr:GetAuthorizationToken",
          "ecr:BatchCheckLayerAvailability",
          "ecr:GetDownloadUrlForLayer",
          "ecr:BatchGetImage",
          "logs:CreateLogStream",
          "logs:PutLogEvents",
        ]
        Resource = "*"
      }],
      length(local.task_secret_arns) > 0 ? [{
        Effect   = "Allow"
        Action   = "ssm:GetParameters"
        Resource = local.task_secret_arns
      }] : [],
    )
  })
}

# EventBridge Scheduler uses this to RunTask + PassRole.
# Each task-type file attaches its own scoped policy.
resource "aws_iam_role" "task_scheduler" {
  count = local.deploy_tasks

  name = "${local.cluster_name}-scheduler"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "scheduler.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
}

# All tasks share the scheduler role, so their statements share one policy.
resource "aws_iam_role_policy" "task_scheduler" {
  count = local.deploy_tasks

  name = "${local.cluster_name}-scheduler"
  role = aws_iam_role.task_scheduler[0].id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "ecs:RunTask"
        Resource = [for k, _ in var.tasks : aws_ecs_task_definition.task[k].arn]
      },
      {
        Effect = "Allow"
        Action = "iam:PassRole"
        Resource = concat(
          [aws_iam_role.task_execution[0].arn],
          [for k, _ in var.tasks : aws_iam_role.task[k].arn],
        )
      },
    ]
  })
}

resource "aws_iam_role" "task" {
  for_each = var.tasks
  name     = "${local.stack}-${each.key}-task"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ecs-tasks.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy" "task" {
  for_each = {
    for k, v in var.tasks : k => v
    if length(v.policy_statements) > 0
  }

  name = "${local.stack}-${each.key}-task"
  role = aws_iam_role.task[each.key].id

  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = each.value.policy_statements
  })
}

# One group for all tasks; streams stay separated by awslogs-stream-prefix.
resource "aws_cloudwatch_log_group" "task" {
  count             = length(var.tasks) > 0 ? 1 : 0
  name              = "/aws/ecs/${local.stack}"
  retention_in_days = 7
}

resource "aws_ecs_task_definition" "task" {
  for_each = var.tasks

  family                   = "${local.stack}-${each.key}"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = each.value.cpu
  memory                   = each.value.mem
  execution_role_arn       = aws_iam_role.task_execution[0].arn
  task_role_arn            = aws_iam_role.task[each.key].arn

  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "ARM64"
  }

  container_definitions = jsonencode([{
    name        = each.key
    image       = each.value.image
    essential   = true
    command     = each.value.command
    environment = each.value.environment
    secrets     = each.value.secrets
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        awslogs-group         = aws_cloudwatch_log_group.task[0].name
        awslogs-region        = local.region
        awslogs-stream-prefix = each.key
      }
    }
  }])
}

resource "aws_scheduler_schedule" "task" {
  for_each = { for k, v in var.tasks : k => v if v.enabled }

  name       = "${local.stack}-${each.key}"
  group_name = "default"

  flexible_time_window {
    mode = "OFF"
  }

  schedule_expression = each.value.schedule

  target {
    arn      = aws_ecs_cluster.this[0].arn
    role_arn = aws_iam_role.task_scheduler[0].arn

    ecs_parameters {
      task_definition_arn = aws_ecs_task_definition.task[each.key].arn
      launch_type         = "FARGATE"
      task_count          = 1

      network_configuration {
        subnets          = data.aws_subnets.default[0].ids
        security_groups  = [data.aws_security_group.default[0].id]
        assign_public_ip = true
      }
    }
  }
}

# Surface task failures via the existing email topic.
resource "aws_cloudwatch_event_rule" "task_failures" {
  count = local.email_alarms_enabled && local.deploy_tasks > 0 ? 1 : 0

  name        = "${local.cluster_name}-failures"
  description = "ECS task failures in ${aws_ecs_cluster.this[0].name}"

  event_pattern = jsonencode({
    source        = ["aws.ecs"]
    "detail-type" = ["ECS Task State Change"]
    detail = {
      clusterArn = [aws_ecs_cluster.this[0].arn]
      lastStatus = ["STOPPED"]
      "$or" = [
        { stopCode = ["TaskFailedToStart"] },
        { containers = { exitCode = [{ "anything-but" = [0] }] } },
      ]
    }
  })
}

resource "aws_cloudwatch_event_target" "task_failures" {
  count = local.email_alarms_enabled && local.deploy_tasks > 0 ? 1 : 0

  rule = aws_cloudwatch_event_rule.task_failures[0].name
  arn  = aws_sns_topic.email_notification[0].arn
}

# Topic policy for the email topic.
resource "aws_sns_topic_policy" "task_failures_publish" {
  count = local.email_alarms_enabled ? 1 : 0

  arn = aws_sns_topic.email_notification[0].arn

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = concat(
      [{
        Sid       = "AllowSameAccountServices"
        Effect    = "Allow"
        Principal = { AWS = "*" }
        Action    = "sns:Publish"
        Resource  = aws_sns_topic.email_notification[0].arn
        Condition = {
          StringEquals = { "AWS:SourceOwner" = local.account_id }
        }
      }],
      local.deploy_tasks > 0 ? [{
        Sid       = "AllowEventBridgeTaskFailures"
        Effect    = "Allow"
        Principal = { Service = "events.amazonaws.com" }
        Action    = "sns:Publish"
        Resource  = aws_sns_topic.email_notification[0].arn
        Condition = {
          ArnEquals = { "aws:SourceArn" = aws_cloudwatch_event_rule.task_failures[0].arn }
        }
      }] : [],
    )
  })
}
