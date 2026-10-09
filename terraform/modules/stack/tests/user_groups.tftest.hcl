# Plan only, with local policy generation and no AWS API calls.
provider "aws" {
  region                      = "us-west-2"
  access_key                  = "testing"
  secret_key                  = "testing"
  skip_credentials_validation = true
  skip_metadata_api_check     = true
  skip_requesting_account_id  = true
  skip_region_validation      = true
}

override_data {
  target = data.aws_caller_identity.current
  values = {
    account_id = "123456789012"
  }
}

override_data {
  target = data.aws_cloudfront_cache_policy.caching_optimized
  values = {
    id = "658327ea-f89d-4fab-a63d-7e88639e58f6"
  }
}

run "restricted_users_deny_reserved_buckets" {
  command = plan

  variables {
    stack = "dcp-client1"
  }

  assert {
    condition = length(setsubtract(
      ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
      toset(flatten([
        for statement in jsondecode(data.aws_iam_policy_document.user_groups["restricted_users"].json).Statement :
        statement.Action
        if statement.Effect == "Deny" && toset(flatten([statement.Resource])) == toset([
          "arn:aws:s3:::dcp-client1-*-repl",
          "arn:aws:s3:::dcp-client1-*-repl/*",
        ])
      ]))
    )) == 0
    error_message = "Restricted users must deny read, write and delete on replication buckets."
  }

  assert {
    condition = length(setsubtract(
      ["s3:PutObject", "s3:DeleteObject"],
      toset(flatten([
        for statement in jsondecode(data.aws_iam_policy_document.user_groups["restricted_users"].json).Statement :
        statement.Action
        if statement.Effect == "Deny" && toset(flatten([statement.Resource])) == toset([
          "arn:aws:s3:::dcp-client1-managed",
          "arn:aws:s3:::dcp-client1-managed/*",
        ])
      ]))
    )) == 0
    error_message = "Restricted users must deny write and delete on the managed bucket."
  }

  assert {
    condition = alltrue([
      for statement in jsondecode(data.aws_iam_policy_document.user_groups["restricted_users"].json).Statement :
      !contains(flatten([statement.Action]), "s3:GetObject") || toset(flatten([statement.Resource])) != toset([
        "arn:aws:s3:::dcp-client1-managed",
        "arn:aws:s3:::dcp-client1-managed/*",
      ])
    ])
    error_message = "Restricted users must not deny read on the managed bucket; it stays readable when explicitly assigned."
  }

  assert {
    condition = alltrue([
      for statement in jsondecode(data.aws_iam_policy_document.user_groups["restricted_users"].json).Statement :
      statement.Effect != "Allow" || toset(flatten([statement.Action])) == toset(["s3:ListAllMyBuckets"])
    ])
    error_message = "Restricted users must have no Allow statements on stack buckets, only ListAllMyBuckets."
  }

  assert {
    condition = length(setsubtract(
      ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
      toset(flatten([
        for statement in jsondecode(data.aws_iam_policy_document.user_groups["power_users"].json).Statement :
        statement.Action
        if statement.Effect == "Deny" && toset(flatten([statement.Resource])) == toset([
          "arn:aws:s3:::dcp-client1-*-repl",
          "arn:aws:s3:::dcp-client1-*-repl/*",
        ])
      ]))
    )) == 0
    error_message = "Power users' replication bucket denies must be unchanged."
  }

  assert {
    condition = length(setsubtract(
      ["s3:GetObject", "s3:PutObject"],
      toset(flatten([
        for statement in jsondecode(data.aws_iam_policy_document.user_groups["standard_users"].json).Statement :
        statement.Action
        if statement.Effect == "Deny" && toset(flatten([statement.Resource])) == toset([
          "arn:aws:s3:::dcp-client1-*-repl",
          "arn:aws:s3:::dcp-client1-*-repl/*",
        ])
      ]))
    )) == 0
    error_message = "Standard users' replication bucket denies must be unchanged."
  }
}
