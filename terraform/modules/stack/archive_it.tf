# Archive-It tasks
locals {
  archive_it_enabled      = var.archive_it_enabled
  archive_it_image        = "public.ecr.aws/docker/library/alpine:latest"
  archive_it_password_arn = "arn:aws:ssm:${local.region}:${local.account_id}:parameter/archive-it/password"
  archive_it_username_arn = "arn:aws:ssm:${local.region}:${local.account_id}:parameter/archive-it/username"
  archive_it_tasks = {
    audit = {
      cpu               = 256
      mem               = 512
      command           = ["sh", "-c", "echo audit"]
      schedule          = "rate(1 hour)"
      policy_statements = []
    }
    inventory = {
      cpu      = 256
      mem      = 512
      command  = ["sh", "-c", "echo inventory"]
      schedule = "rate(1 hour)"
      policy_statements = [
        {
          Effect   = "Allow"
          Action   = "s3:ListBucket"
          Resource = "arn:aws:s3:::${local.stack}${local.archive_it_suffix}"
        },
        {
          Effect   = "Allow"
          Action   = ["s3:GetObject", "s3:PutObject"]
          Resource = "${aws_s3_bucket.main["managed"].arn}/${local.archive_it_prefix}/*"
        },
      ]
    }
    sync = {
      cpu               = 256
      mem               = 512
      command           = ["sh", "-c", "echo sync"]
      schedule          = "rate(1 hour)"
      policy_statements = []
    }
  }
}

# Per-task role — container's AWS API calls. S3/etc. perms attach here.
resource "aws_iam_role" "archive_it" {
  for_each = local.archive_it_tasks

  name = "${local.stack}-${each.key}-task"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ecs-tasks.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy" "archive_it" {
  for_each = {
    for k, v in local.archive_it_tasks : k => v
    if length(v.policy_statements) > 0
  }

  name = "${local.stack}-${each.key}-task"
  role = aws_iam_role.archive_it[each.key].id

  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = each.value.policy_statements
  })
}

resource "aws_cloudwatch_log_group" "archive_it" {
  name              = "/aws/ecs/${local.stack}-archive-it"
  retention_in_days = 7
}

resource "aws_ecs_task_definition" "archive_it" {
  for_each = local.archive_it_tasks

  family                   = "${local.stack}-${each.key}"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = each.value.cpu
  memory                   = each.value.mem
  execution_role_arn       = aws_iam_role.execution.arn
  task_role_arn            = aws_iam_role.archive_it[each.key].arn

  container_definitions = jsonencode([
    {
      name      = each.key
      image     = local.archive_it_image
      essential = true
      command   = each.value.command
      secrets = [
        {
          name      = "ARCHIVE_IT_PASSWORD"
          valueFrom = local.archive_it_password_arn
        },
        {
          name      = "ARCHIVE_IT_USERNAME"
          valueFrom = local.archive_it_username_arn
        },
      ]
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          awslogs-group         = aws_cloudwatch_log_group.archive_it.name
          awslogs-region        = data.aws_region.current.region
          awslogs-stream-prefix = each.key
        }
      }
    }
  ])
}

resource "aws_iam_role_policy" "archive_it_execution_secrets" {
  name = "${local.stack}-archive-it-secrets"
  role = aws_iam_role.execution.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = "ssm:GetParameters"
      Resource = [
        local.archive_it_password_arn,
        local.archive_it_username_arn,
      ]
    }]
  })
}

resource "aws_iam_role_policy" "archive_it_scheduler" {
  for_each = local.archive_it_tasks

  name = "${local.stack}-${each.key}-scheduler"
  role = aws_iam_role.task_scheduler.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "ecs:RunTask"
        Resource = aws_ecs_task_definition.archive_it[each.key].arn
      },
      {
        Effect = "Allow"
        Action = "iam:PassRole"
        Resource = [
          aws_iam_role.execution.arn,
          aws_iam_role.archive_it[each.key].arn,
        ]
      }
    ]
  })
}

resource "aws_scheduler_schedule" "archive_it" {
  for_each = {
    for k, v in local.archive_it_tasks : k => v
    if local.archive_it_enabled
  }

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
      task_definition_arn = aws_ecs_task_definition.archive_it[each.key].arn
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
