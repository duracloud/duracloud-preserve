# Core resources for scheduled ECS Fargate tasks
locals {
  cluster_name = "${local.stack}-tasks"

  # Internally-defined + externally-passed tasks merged into one map.
  tasks = merge(local.archive_it_tasks, var.tasks)

  # Union of all secret ARNs across tasks, for the shared execution role.
  task_secret_arns = distinct(flatten([
    for t in local.tasks : [for s in t.secrets : s.valueFrom]
  ]))

}

data "aws_vpc" "default" {
  default = true
}

data "aws_subnets" "default" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.default.id]
  }
}

data "aws_security_group" "default" {
  vpc_id = data.aws_vpc.default.id
  name   = "default"
}

resource "aws_ecs_cluster" "this" {
  name = local.cluster_name
}

# Fargate uses this to pull images and write logs. Shared by all tasks.
resource "aws_iam_role" "execution" {
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

resource "aws_iam_role_policy_attachment" "execution" {
  role       = aws_iam_role.execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

# EventBridge Scheduler uses this to RunTask + PassRole.
# Each task-type file attaches its own scoped policy.
resource "aws_iam_role" "task_scheduler" {
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

# Surface task failures via the existing email topic.
resource "aws_cloudwatch_event_rule" "task_failures" {
  count = local.email_alarms_enabled ? 1 : 0

  name        = "${local.cluster_name}-failures"
  description = "ECS task failures in ${aws_ecs_cluster.this.name}"

  event_pattern = jsonencode({
    source        = ["aws.ecs"]
    "detail-type" = ["ECS Task State Change"]
    detail = {
      clusterArn = [aws_ecs_cluster.this.arn]
      lastStatus = ["STOPPED"]
      "$or" = [
        { stopCode = ["TaskFailedToStart"] },
        { containers = { exitCode = [{ "anything-but" = [0] }] } },
      ]
    }
  })
}

resource "aws_cloudwatch_event_target" "task_failures_sns" {
  count = local.email_alarms_enabled ? 1 : 0

  rule = aws_cloudwatch_event_rule.task_failures[0].name
  arn  = aws_sns_topic.email_notification[0].arn
}

resource "aws_iam_role" "task" {
  for_each = local.tasks
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
    for k, v in local.tasks : k => v
    if length(v.policy_statements) > 0
  }

  name = "${local.stack}-${each.key}-task"
  role = aws_iam_role.task[each.key].id

  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = each.value.policy_statements
  })
}

resource "aws_cloudwatch_log_group" "task" {
  for_each          = local.tasks
  name              = "/aws/ecs/${local.stack}-${each.key}"
  retention_in_days = 7
}

resource "aws_ecs_task_definition" "task" {
  for_each = local.tasks

  family                   = "${local.stack}-${each.key}"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = each.value.cpu
  memory                   = each.value.mem
  execution_role_arn       = aws_iam_role.execution.arn
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
        awslogs-group         = aws_cloudwatch_log_group.task[each.key].name
        awslogs-region        = local.region
        awslogs-stream-prefix = each.key
      }
    }
  }])
}

resource "aws_iam_role_policy" "task_execution_secrets" {
  count = length(local.task_secret_arns) > 0 ? 1 : 0

  name = "${local.stack}-task-secrets"
  role = aws_iam_role.execution.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = "ssm:GetParameters"
      Resource = local.task_secret_arns
    }]
  })
}

resource "aws_iam_role_policy" "task_scheduler" {
  for_each = local.tasks

  name = "${local.stack}-${each.key}-scheduler"
  role = aws_iam_role.task_scheduler.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "ecs:RunTask"
        Resource = aws_ecs_task_definition.task[each.key].arn
      },
      {
        Effect = "Allow"
        Action = "iam:PassRole"
        Resource = [
          aws_iam_role.execution.arn,
          aws_iam_role.task[each.key].arn,
        ]
      },
    ]
  })
}

resource "aws_scheduler_schedule" "task" {
  for_each = { for k, v in local.tasks : k => v if v.enabled }

  name       = "${local.stack}-${each.key}"
  group_name = "default"

  flexible_time_window {
    mode = "OFF"
  }

  schedule_expression = each.value.schedule

  target {
    arn      = aws_ecs_cluster.this.arn
    role_arn = aws_iam_role.task_scheduler.arn

    ecs_parameters {
      task_definition_arn = aws_ecs_task_definition.task[each.key].arn
      launch_type         = "FARGATE"
      task_count          = 1

      network_configuration {
        subnets          = data.aws_subnets.default.ids
        security_groups  = [data.aws_security_group.default.id]
        assign_public_ip = true
      }
    }
  }
}

# EventBridge needs explicit Publish permission on the topic
resource "aws_sns_topic_policy" "task_failures_publish" {
  count = local.email_alarms_enabled ? 1 : 0

  arn = aws_sns_topic.email_notification[0].arn

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid       = "AllowSameAccountServices"
        Effect    = "Allow"
        Principal = { AWS = "*" }
        Action    = "sns:Publish"
        Resource  = aws_sns_topic.email_notification[0].arn
        Condition = {
          StringEquals = { "AWS:SourceOwner" = local.account_id }
        }
      },
      {
        Sid       = "AllowEventBridgeTaskFailures"
        Effect    = "Allow"
        Principal = { Service = "events.amazonaws.com" }
        Action    = "sns:Publish"
        Resource  = aws_sns_topic.email_notification[0].arn
        Condition = {
          ArnEquals = { "aws:SourceArn" = aws_cloudwatch_event_rule.task_failures[0].arn }
        }
      },
    ]
  })
}
