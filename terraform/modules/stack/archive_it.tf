# Archive-It scheduled tasks — entries plug into local.tasks in tasks.tf.
locals {
  archive_it_image        = "duracloud/dcp:latest"
  archive_it_password_arn = "arn:aws:ssm:${local.region}:${local.account_id}:parameter/archive-it/password"
  archive_it_username_arn = "arn:aws:ssm:${local.region}:${local.account_id}:parameter/archive-it/username"

  archive_it_defaults = {
    cpu      = 256
    mem      = 512
    image    = local.archive_it_image
    schedule = "rate(1 hour)"
    enabled  = var.archive_it_enabled
    environment = [
      { name = "STACK", value = local.stack },
    ]
    secrets = [
      { name = "ARCHIVE_IT_PASSWORD", valueFrom = local.archive_it_password_arn },
      { name = "ARCHIVE_IT_USERNAME", valueFrom = local.archive_it_username_arn },
    ]
  }

  archive_it_tasks = {
    "archive-it-audit" = merge(local.archive_it_defaults, {
      command           = ["ait", "audit", "-h"]
      policy_statements = []
    })
    "archive-it-inventory" = merge(local.archive_it_defaults, {
      command = ["ait", "inventory"]
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
    })
    "archive-it-sync" = merge(local.archive_it_defaults, {
      command           = ["ait", "sync", "-h"]
      policy_statements = []
    })
  }
}
