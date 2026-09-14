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

mock_provider "sftpgo" {}

override_data {
  target = data.aws_iam_group.user
  values = { group_name = "test-group" }
}

run "bucket_permissions" {
  command = plan

  variables {
    users = {
      scoped = {
        email = "scoped@example.com"
        buckets = [
          "dcp-client1-archives",
          "dcp-client1-public",
          "dcp-client1-managed",
          "dcp-client1-archives-repl",
          "dcp-client2-archives",
          "dcp-client10-archives",
        ]
        memberships = [
          { stack = "dcp-client1", group = "restricted-users", allow_delete = true },
          { stack = "dcp-client2", group = "restricted-users", allow_delete = false },
        ]
      }
      default_restricted = {
        email       = "default@example.com"
        buckets     = ["dcp-client1-archives"]
        memberships = [{ stack = "dcp-client1", group = "restricted-users" }]
      }
      explicit_false = {
        email       = "false@example.com"
        buckets     = ["dcp-client1-archives"]
        memberships = [{ stack = "dcp-client1", group = "restricted-users", allow_delete = false }]
      }
      standard = {
        email       = "standard@example.com"
        buckets     = ["dcp-client1-archives"]
        memberships = [{ stack = "dcp-client1", group = "standard-users" }]
      }
      power = {
        email       = "power@example.com"
        buckets     = ["dcp-client1-archives"]
        memberships = [{ stack = "dcp-client1", group = "power-users" }]
      }
      empty = {
        email       = "empty@example.com"
        memberships = [{ stack = "dcp-client1", group = "restricted-users", allow_delete = true }]
      }
    }
  }

  assert {
    condition = toset(flatten([
      for statement in jsondecode(data.aws_iam_policy_document.s3_access["scoped"].json).Statement : statement.Resource
      if statement.Effect == "Allow" && contains(flatten([statement.Action]), "s3:DeleteObject")
      ])) == toset([
      "arn:aws:s3:::dcp-client1-archives/*",
      "arn:aws:s3:::dcp-client1-public/*",
      "arn:aws:s3:::dcp-client1-managed/*",
      "arn:aws:s3:::dcp-client1-archives-repl/*",
    ])
    error_message = "Deletes must be limited to assigned buckets in opted-in stacks, including the stack delimiter."
  }

  assert {
    condition = alltrue([
      for name in ["default_restricted", "explicit_false", "standard", "power"] : alltrue([
        for statement in jsondecode(data.aws_iam_policy_document.s3_access[name].json).Statement :
        !contains(flatten([statement.Action]), "s3:DeleteObject")
      ])
    ])
    error_message = "Default, false, standard, and power memberships must retain their existing per-user permissions."
  }

  assert {
    condition = toset(flatten([
      for statement in jsondecode(data.aws_iam_policy_document.s3_access["scoped"].json).Statement : statement.Resource
      if statement.Effect == "Deny" && contains(flatten([statement.Action]), "s3:DeleteObject")
      ])) == toset([
      "arn:aws:s3:::dcp-client1-managed",
      "arn:aws:s3:::dcp-client1-managed/*",
      "arn:aws:s3:::dcp-client1-archives-repl",
      "arn:aws:s3:::dcp-client1-archives-repl/*",
    ])
    error_message = "Managed and replication bucket delete denies must remain in the policy."
  }

  assert {
    condition = alltrue([
      for statement in jsondecode(data.aws_iam_policy_document.s3_access["scoped"].json).Statement :
      !contains(flatten([statement.Action]), "s3:DeleteObjectVersion") && !contains(flatten([statement.Action]), "s3:DeleteBucket")
    ])
    error_message = "Opting into object deletes must not grant version or bucket deletion."
  }

  assert {
    condition     = !contains(keys(aws_iam_user_policy.s3_access), "empty")
    error_message = "Opting in without assigned buckets must not create an S3 access policy."
  }
}

run "reject_standard_delete_flag" {
  command = plan

  variables {
    users = {
      invalid = {
        email       = "invalid@example.com"
        memberships = [{ stack = "dcp-client1", group = "standard-users", allow_delete = true }]
      }
    }
  }

  expect_failures = [var.users]
}

run "reject_power_delete_flag" {
  command = plan

  variables {
    users = {
      invalid = {
        email       = "invalid@example.com"
        memberships = [{ stack = "dcp-client1", group = "power-users", allow_delete = true }]
      }
    }
  }

  expect_failures = [var.users]
}
