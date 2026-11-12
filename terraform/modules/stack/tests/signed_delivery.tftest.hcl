# Plan only, with no AWS API calls. Signed URL access and per-path CORS on the
# public CloudFront distribution (public_signed.tf, public.tf). The CORS logic
# itself is tested in public-file-response.test.js.
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
    condition     = length(aws_cloudfront_public_key.signed) == 0 && length(aws_cloudfront_key_group.signed) == 0
    error_message = "No keys or key group are created by default."
  }

  assert {
    condition     = jsonencode(jsondecode(trimsuffix(trimprefix(split("\n", aws_cloudfront_function.public_file_response["public"].code)[0], "var CORS_ORIGINS_BY_PATH = "), ";"))) == "{}"
    error_message = "The response function has no CORS paths by default."
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
      "/open-hls/"   = ["http://localhost:8081", "http://localhost:8080"]
      "/restricted/" = ["http://localhost:8080"]
    }
  }

  assert {
    condition = (
      length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior) == 1 &&
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].path_pattern == "/restricted/*"
    )
    error_message = "Only the signed path gets an ordered behavior; CORS paths don't need one."
  }

  assert {
    condition = alltrue([
      for b in aws_cloudfront_distribution.public["public"].ordered_cache_behavior :
      b.trusted_key_groups == tolist(["KG1"]) &&
      b.cache_policy_id == aws_cloudfront_distribution.public["public"].default_cache_behavior[0].cache_policy_id &&
      b.compress &&
      b.viewer_protocol_policy == "redirect-to-https" &&
      (b.response_headers_policy_id == null || b.response_headers_policy_id == "") &&
      length(b.function_association) == 1 &&
      one([for f in b.function_association : f.event_type]) == "viewer-response"
    ])
    error_message = "/restricted/* requires the key group and has caching_optimized and the response function, with no response headers policy."
  }

  assert {
    condition = jsonencode(jsondecode(trimsuffix(trimprefix(split("\n", aws_cloudfront_function.public_file_response["public"].code)[0], "var CORS_ORIGINS_BY_PATH = "), ";"))) == jsonencode({
      "/open-hls/"   = ["http://localhost:8080", "http://localhost:8081"]
      "/restricted/" = ["http://localhost:8080"]
    })
    error_message = "The response function gets each path's own origins."
  }

  assert {
    condition     = endswith(aws_cloudfront_function.public_file_response["public"].code, file("${path.module}/assets/cloudfront/public-file-response.js"))
    error_message = "The response function is the CORS configuration followed by the unchanged function source."
  }

  assert {
    condition = (
      aws_cloudfront_distribution.public["public"].default_cache_behavior[0].response_headers_policy_id == null ||
      aws_cloudfront_distribution.public["public"].default_cache_behavior[0].response_headers_policy_id == ""
    )
    error_message = "The default behavior has no response headers policy."
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
    condition     = jsonencode(jsondecode(trimsuffix(trimprefix(split("\n", aws_cloudfront_function.public_file_response["public"].code)[0], "var CORS_ORIGINS_BY_PATH = "), ";"))) == "{}"
    error_message = "With no CORS entries, the response function has no CORS paths."
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
    condition     = length(aws_cloudfront_distribution.public["public"].ordered_cache_behavior) == 0
    error_message = "An unsigned CORS path needs no ordered behavior."
  }

  assert {
    condition     = jsonencode(jsondecode(trimsuffix(trimprefix(split("\n", aws_cloudfront_function.public_file_response["public"].code)[0], "var CORS_ORIGINS_BY_PATH = "), ";"))) == jsonencode({ "/open-hls/" = ["https://client.edu"] })
    error_message = "Only the listed path gets CORS."
  }

  assert {
    condition     = length(aws_cloudfront_key_group.signed) == 0
    error_message = "No key group is created."
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
      aws_cloudfront_distribution.public["public"].ordered_cache_behavior[0].path_pattern == "/media/hls/*"
    )
    error_message = "The signed behavior follows the custom base path."
  }

  assert {
    condition     = jsonencode(jsondecode(trimsuffix(trimprefix(split("\n", aws_cloudfront_function.public_file_response["public"].code)[0], "var CORS_ORIGINS_BY_PATH = "), ";"))) == jsonencode({ "/media/hls/" = ["https://client.edu"] })
    error_message = "CORS for the signed path follows the custom base path."
  }

  assert {
    condition     = output.cloudfront_signed_base_url == "https://test.preserve.duracloud.org/media/hls/"
    error_message = "The base URL follows the custom base path."
  }
}

run "function_size_near_limit" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/" = [for i in range(60) : "https://player-${i}.example.edu"]
    }
  }

  assert {
    condition     = length(aws_cloudfront_function.public_file_response["public"].code) <= 10240
    error_message = "A realistic CORS configuration fits within the function size limit."
  }
}

run "reject_function_over_size_limit" {
  command = plan

  variables {
    cloudfront_cors_allowed_origins_by_path = {
      "/open-hls/" = [for i in range(300) : "https://player-${i}.example.edu"]
    }
  }

  expect_failures = [aws_cloudfront_function.public_file_response]
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
