# Archive-It scheduled tasks — output plugs into the stack module's `tasks` input.
data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

locals {
  account_id = data.aws_caller_identity.current.account_id
  region     = data.aws_region.current.region

  # TODO: construct via input vars with defaults?
  password_arn = "arn:aws:ssm:${local.region}:${local.account_id}:parameter/archive-it/password"
  username_arn = "arn:aws:ssm:${local.region}:${local.account_id}:parameter/archive-it/username"

  source_bucket_arn  = "arn:aws:s3:::${var.stack}${local.archive_it_suffix}"
  managed_bucket_arn = "arn:aws:s3:::${var.stack}${local.managed_suffix}"

  base = {
    cpu      = 256
    mem      = 512
    image    = "duracloud/dcp:latest"
    schedule = "rate(1 day)"
    enabled  = true
  }

  # Drop unset (null) fields so they don't clobber `base` on merge.
  overrides = {
    for k, v in var.config :
    k => { for fk, fv in v : fk => fv if fv != null }
  }

  settings = {
    for name in ["audit", "inventory", "sync"] :
    name => merge(local.base, lookup(local.overrides, name, {}))
  }

  common = {
    environment = [
      { name = "STACK", value = var.stack },
    ]
    secrets = [
      { name = "ARCHIVE_IT_PASSWORD", valueFrom = local.password_arn },
      { name = "ARCHIVE_IT_USERNAME", valueFrom = local.username_arn },
    ]
  }

  tasks = {
    "archive-it-audit" = merge(local.settings.audit, local.common, {
      command = concat(
        ["ait", "audit"],
        var.expiration == null ? [] : ["--expire-after-years", tostring(var.expiration)],
      )
      environment = concat(
        local.common.environment,
        [
          { name = "EXPIRE_TAG_KEY", value = local.lifecycle_legacy_duracloud_file_tag_key },
          { name = "EXPIRE_TAG_VALUE", value = local.lifecycle_legacy_duracloud_file_tag_val },
        ],
      )
      policy_statements = [
        {
          Effect   = "Allow"
          Action   = ["s3:ListBucket"]
          Resource = local.source_bucket_arn
        },
        {
          Effect   = "Allow"
          Action   = ["s3:GetObject", "s3:PutObjectTagging"]
          Resource = "${local.source_bucket_arn}/*"
        },
        {
          Effect   = "Allow"
          Action   = ["s3:GetObject", "s3:PutObject"]
          Resource = "${local.managed_bucket_arn}/${local.archive_it_prefix}/*"
        },
      ]
    })
    "archive-it-inventory" = merge(local.settings.inventory, local.common, {
      command = ["ait", "inventory"]
      policy_statements = [
        {
          Effect   = "Allow"
          Action   = ["s3:ListBucket"]
          Resource = local.source_bucket_arn
        },
        {
          Effect   = "Allow"
          Action   = ["s3:GetObject", "s3:PutObject"]
          Resource = "${local.managed_bucket_arn}/${local.archive_it_prefix}/*"
        },
      ]
    })
    "archive-it-sync" = merge(local.settings.sync, local.common, {
      command = ["ait", "sync"]
      policy_statements = [
        {
          Effect   = "Allow"
          Action   = ["s3:ListBucket"]
          Resource = local.source_bucket_arn
        },
        {
          Effect   = "Allow"
          Action   = ["s3:GetObject", "s3:PutObject"]
          Resource = "${local.source_bucket_arn}/*"
        },
        {
          Effect   = "Allow"
          Action   = ["s3:GetObject", "s3:DeleteObject"]
          Resource = "${local.managed_bucket_arn}/${local.archive_it_prefix}/*"
        },
      ]
    })
  }
}
