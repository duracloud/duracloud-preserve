# Core resources for scheduled ECS Fargate tasks
locals {
  cluster_name = "${local.stack}-tasks"
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

# EventBridge needs explicit Publish permission on the topic
resource "aws_sns_topic_policy" "task_failures_publish" {
  count = local.email_alarms_enabled ? 1 : 0

  arn = aws_sns_topic.email_notification[0].arn

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "events.amazonaws.com" }
      Action    = "sns:Publish"
      Resource  = aws_sns_topic.email_notification[0].arn
      Condition = {
        ArnEquals = {
          "aws:SourceArn" = aws_cloudwatch_event_rule.task_failures[0].arn
        }
      }
    }]
  })
}
