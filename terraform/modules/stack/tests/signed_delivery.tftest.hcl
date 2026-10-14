# Plan only, with no AWS API calls. Signed URL access and per-path CORS on the
# public CloudFront distribution (public_signed.tf, public.tf).
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

# id is an input argument on this data source, so it can't be overridden and
# is null during plan. Behaviors are checked against the default behavior's
# cache policy instead.
override_data {
  target = data.aws_cloudfront_cache_policy.caching_optimized
  values = {
    etag = "E1"
  }
}

# Known IDs during plan so behaviors can be checked against them
override_resource {
  target          = aws_cloudfront_public_key.signed
  override_during = plan
  values = {
    id = "K1"
  }
}

override_resource {
  target          = aws_cloudfront_key_group.signed
  override_during = plan
  values = {
    id = "KG1"
  }
}

# A distinct ID per path, so a behavior wired to another path's policy fails
override_resource {
  target          = aws_cloudfront_response_headers_policy.cors["/open-hls/"]
  override_during = plan
  values = {
    id = "RHP-OPEN-HLS"
  }
}

override_resource {
  target          = aws_cloudfront_response_headers_policy.cors["/restricted/"]
  override_during = plan
  values = {
    id = "RHP-RESTRICTED"
  }
}

override_resource {
  target          = aws_cloudfront_response_headers_policy.cors["/media/hls/"]
  override_during = plan
  values = {
    id = "RHP-MEDIA-HLS"
  }
}

override_resource {
  target          = aws_cloudfront_distribution.public
  override_during = plan
  values = {
    domain_name = "d111111abcdef8.cloudfront.net"
  }
}

variables {
  stack              = "dcp-test"
  cloudfront_enabled = true

  # Plan only: the PEM is never parsed
  test_pem = "-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAtest\n-----END PUBLIC KEY-----\n"
}

run "disabled_by_default" {
  command = plan

  assert {
    condition     = length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior) == 0
    error_message = "No ordered behaviors are created by default."
  }

  assert {
    condition = (
      length(aws_cloudfront_public_key.signed) == 0 &&
      length(aws_cloudfront_key_group.signed) == 0 &&
      length(aws_cloudfront_response_headers_policy.cors) == 0
    )
    error_message = "No keys, key group, or CORS policies are created by default."
  }

  assert {
    condition = (
      aws_cloudfront_distribution.public["public"].default_cache_behavior[0].response_headers_policy_id == null ||
      aws_cloudfront_distribution.public["public"].default_cache_behavior[0].response_headers_policy_id == ""
    )
    error_message = "The default behavior has no response headers policy."
  }

  assert {
    condition     = output.cloudfront_signed_base_url == null && length(output.cloudfront_signed_key_ids) == 0
    error_message = "Signed outputs are empty by default."
  }
}

run "keys_only" {
  command = plan

  variables {
    cloudfront_signed_public_keys = { test_1 = var.test_pem }
  }

  assert {
    condition     = length(aws_cloudfront_public_key.signed) == 1 && length(aws_cloudfront_key_group.signed) == 1
    error_message = "Keys and the key group can be registered before signing is enabled."
  }

  assert {
    condition     = aws_cloudfront_key_group.signed["public"].items == toset(["K1"])
    error_message = "The key group contains the registered key."
  }

  assert {
    condition     = startswith(aws_cloudfront_public_key.signed["test_1"].name_prefix, "dcp-test-test_1-")
    error_message = "Public keys use a name prefix so a replacement can be created before the old key is destroyed."
  }

  assert {
    condition     = length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior) == 0
    error_message = "No ordered behavior until signing is enabled."
  }

  assert {
    condition     = output.cloudfront_signed_base_url == null && output.cloudfront_signed_key_ids == { test_1 = "K1" }
    error_message = "Key IDs are output before signing is enabled; the base URL is not."
  }
}

run "signed_with_per_path_cors" {
  command = plan

  variables {
    acm_cert_arn                  = "arn:aws:acm:us-east-1:123456789012:certificate/test"
    cloudfront_domain             = "preserve.duracloud.org"
    cloudfront_signed_enabled     = true
    cloudfront_signed_public_keys = { test_1 = var.test_pem }
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/"   = ["http://localhost:8080", "http://localhost:8081"]
      "/restricted/" = ["http://localhost:8080"]
    }
  }

  assert {
    condition     = length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior) == 2
    error_message = "One signed behavior and one unsigned CORS behavior."
  }

  assert {
    condition = alltrue([
      for b in aws_cloudfront_distribution.public["public"].ordered_cache_behavior :
      b.trusted_key_groups == tolist(["KG1"]) &&
      b.cache_policy_id == aws_cloudfront_distribution.public["public"].default_cache_behavior[0].cache_policy_id &&
      b.compress &&
      b.viewer_protocol_policy == "redirect-to-https" &&
      b.response_headers_policy_id == "RHP-RESTRICTED" &&
      length(b.function_association) == 1 &&
      one([for f in b.function_association : f.event_type]) == "viewer-response"
      if b.path_pattern == "/restricted/*"
    ]) && length([for b in aws_cloudfront_distribution.public["public"].ordered_cache_behavior : b if b.path_pattern == "/restricted/*"]) == 1
    error_message = "/restricted/* requires the key group and has caching_optimized, the response function, and its CORS policy."
  }

  assert {
    condition = alltrue([
      for b in aws_cloudfront_distribution.public["public"].ordered_cache_behavior :
      try(length(b.trusted_key_groups), 0) == 0 &&
      b.response_headers_policy_id == "RHP-OPEN-HLS" &&
      length(b.function_association) == 1
      if b.path_pattern == "/open-hls/*"
    ]) && length([for b in aws_cloudfront_distribution.public["public"].ordered_cache_behavior : b if b.path_pattern == "/open-hls/*"]) == 1
    error_message = "/open-hls/* is unsigned, with its CORS policy and the response function."
  }

  assert {
    condition = (
      aws_cloudfront_response_headers_policy.cors["/open-hls/"].cors_config[0].access_control_allow_origins[0].items == toset(["http://localhost:8080", "http://localhost:8081"]) &&
      aws_cloudfront_response_headers_policy.cors["/restricted/"].cors_config[0].access_control_allow_origins[0].items == toset(["http://localhost:8080"])
    )
    error_message = "Each CORS policy contains only its own path's origins."
  }

  assert {
    condition = alltrue([
      for p in aws_cloudfront_response_headers_policy.cors : (
        p.cors_config[0].access_control_allow_methods[0].items == toset(["GET", "HEAD"]) &&
        p.cors_config[0].access_control_expose_headers[0].items == toset(["Accept-Ranges", "Content-Range", "ETag"]) &&
        !p.cors_config[0].access_control_allow_credentials &&
        p.cors_config[0].origin_override &&
        length(p.name) <= 128
      )
    ])
    error_message = "CORS policies allow GET/HEAD without credentials, expose range headers, and override the origin."
  }

  assert {
    condition = (
      aws_cloudfront_distribution.public["public"].default_cache_behavior[0].response_headers_policy_id == null ||
      aws_cloudfront_distribution.public["public"].default_cache_behavior[0].response_headers_policy_id == ""
    )
    error_message = "The default behavior still has no CORS."
  }

  assert {
    condition     = output.cloudfront_signed_base_url == "https://test.preserve.duracloud.org/restricted/"
    error_message = "The base URL uses the custom domain and the signed path."
  }
}

run "signed_without_cors" {
  command = plan

  variables {
    cloudfront_signed_enabled     = true
    cloudfront_signed_public_keys = { test_1 = var.test_pem }
  }

  assert {
    condition = (
      length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior) == 1 &&
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].path_pattern == "/restricted/*" &&
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].trusted_key_groups == tolist(["KG1"])
    )
    error_message = "Signing works without a CORS entry."
  }

  assert {
    condition = (
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].response_headers_policy_id == null ||
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].response_headers_policy_id == ""
    )
    error_message = "With no CORS entry for the signed path, the signed behavior has no response headers policy."
  }

  assert {
    condition     = output.cloudfront_signed_base_url == "https://d111111abcdef8.cloudfront.net/restricted/"
    error_message = "Without a custom domain, the base URL uses the distribution domain."
  }
}

run "cors_open_path_only" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/" = ["https://client.edu"]
    }
  }

  assert {
    condition = (
      length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior) == 1 &&
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].path_pattern == "/open-hls/*" &&
      try(length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].trusted_key_groups), 0) == 0 &&
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].response_headers_policy_id == "RHP-OPEN-HLS"
    )
    error_message = "One unsigned /open-hls/* behavior with its CORS policy."
  }

  assert {
    condition     = length(aws_cloudfront_response_headers_policy.cors) == 1 && length(aws_cloudfront_key_group.signed) == 0
    error_message = "Only the listed path gets a CORS policy, and no key group is created."
  }
}

run "custom_base_path" {
  command = plan

  variables {
    acm_cert_arn                  = "arn:aws:acm:us-east-1:123456789012:certificate/test"
    cloudfront_domain             = "preserve.duracloud.org"
    cloudfront_signed_enabled     = true
    cloudfront_signed_base_path   = "/media/hls/"
    cloudfront_signed_public_keys = { test_1 = var.test_pem }
    cloudfront_cors_allowed_origins_by_path = {
      "/media/hls/" = ["https://client.edu"]
    }
  }

  assert {
    condition = (
      length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior) == 1 &&
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].path_pattern == "/media/hls/*" &&
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].response_headers_policy_id == "RHP-MEDIA-HLS"
    )
    error_message = "The signed behavior follows the custom base path."
  }

  assert {
    condition     = output.cloudfront_signed_base_url == "https://test.preserve.duracloud.org/media/hls/"
    error_message = "The base URL follows the custom base path."
  }
}

# Rejections

run "reject_base_path_root" {
  command = plan
  variables { cloudfront_signed_base_path = "/" }
  expect_failures = [var.cloudfront_signed_base_path]
}

run "reject_base_path_no_slashes" {
  command = plan
  variables { cloudfront_signed_base_path = "restricted" }
  expect_failures = [var.cloudfront_signed_base_path]
}

run "reject_base_path_empty_segment" {
  command = plan
  variables { cloudfront_signed_base_path = "/a//b/" }
  expect_failures = [var.cloudfront_signed_base_path]
}

run "reject_base_path_dot_segment" {
  command = plan
  variables { cloudfront_signed_base_path = "/../" }
  expect_failures = [var.cloudfront_signed_base_path]
}

run "reject_base_path_wildcard" {
  command = plan
  variables { cloudfront_signed_base_path = "/r*/" }
  expect_failures = [var.cloudfront_signed_base_path]
}

run "reject_base_path_query" {
  command = plan
  variables { cloudfront_signed_base_path = "/r?x/" }
  expect_failures = [var.cloudfront_signed_base_path]
}

run "reject_cors_path_root" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_cors_path_no_slashes" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "restricted" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_cors_path_empty_segment" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/a//b/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_cors_path_dot_segment" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/../" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_cors_path_wildcard" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/r*/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_cors_path_query" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/r?x/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_nested_cors_paths" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/"     = ["https://client.edu"]
      "/open-hls/sub/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_cors_path_nested_in_signed_path" {
  command = plan

  variables {
    cloudfront_signed_enabled     = true
    cloudfront_signed_public_keys = { test_1 = var.test_pem }
    cloudfront_cors_allowed_origins_by_path = {
      "/restricted/closed-hls/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_cors_path_containing_signed_path" {
  command = plan

  variables {
    cloudfront_signed_enabled     = true
    cloudfront_signed_base_path   = "/restricted/hls/"
    cloudfront_signed_public_keys = { test_1 = var.test_pem }
    cloudfront_cors_allowed_origins_by_path = {
      "/restricted/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_cors_for_signed_path_while_disabled" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/restricted/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_empty_origins" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/" = []
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_http_non_localhost_origin" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/" = ["http://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_origin_with_path" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/" = ["https://client.edu/player"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}

run "reject_too_many_keys" {
  command = plan

  variables {
    cloudfront_signed_public_keys = {
      k1 = var.test_pem
      k2 = var.test_pem
      k3 = var.test_pem
      k4 = var.test_pem
      k5 = var.test_pem
      k6 = var.test_pem
    }
  }

  expect_failures = [var.cloudfront_signed_public_keys]
}

run "reject_private_key" {
  command = plan

  variables {
    cloudfront_signed_public_keys = {
      oops = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBg\n-----END PRIVATE KEY-----\n"
    }
  }

  expect_failures = [var.cloudfront_signed_public_keys]
}

run "reject_invalid_key_name" {
  command = plan

  variables {
    cloudfront_signed_public_keys = { "bad name" = var.test_pem }
  }

  expect_failures = [var.cloudfront_signed_public_keys]
}

run "reject_signed_without_keys" {
  command = plan
  variables { cloudfront_signed_enabled = true }
  expect_failures = [var.cloudfront_signed_enabled]
}

run "reject_keys_without_cloudfront" {
  command = plan

  variables {
    cloudfront_enabled            = false
    cloudfront_signed_public_keys = { test_1 = var.test_pem }
  }

  expect_failures = [var.cloudfront_signed_public_keys]
}

run "reject_cors_without_cloudfront" {
  command = plan

  variables {
    cloudfront_enabled = false
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/" = ["https://client.edu"]
    }
  }

  expect_failures = [var.cloudfront_cors_allowed_origins_by_path]
}
