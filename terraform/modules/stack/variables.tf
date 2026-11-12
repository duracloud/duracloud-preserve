variable "billing_alert_threshold" {
  description = "Trigger billing alert when threshold is exceeded"
  type        = number
  default     = null

  validation {
    condition     = var.billing_alert_threshold == null || var.billing_alert_threshold > 0
    error_message = "billing_alert_threshold must be greater than 0 when set."
  }
}

variable "acm_cert_arn" {
  description = "ARN of a pre-existing ACM certificate (in us-east-1) to use with CloudFront"
  type        = string
  default     = null
}

variable "org_account_id" {
  description = "Optional AWS organization account id granted read access to the LATEST storage stats and reports"
  type        = string
  default     = null

  validation {
    condition     = var.org_account_id == null || can(regex("^[0-9]{12}$", var.org_account_id))
    error_message = "org_account_id must be a 12 digit AWS account id when set."
  }
}

variable "cloudfront_enabled" {
  description = "Enable CloudFront and its public bucket pair for public file access"
  type        = bool
  default     = false
}

variable "cloudfront_domain" {
  description = "The domain that will be used with CloudFront for public file access"
  type        = string
  default     = ""
}

variable "cloudfront_geo_restriction_type" {
  description = "The type of geo restriction to use for CloudFront"
  type        = string
  default     = "none"

  validation {
    condition     = contains(["none", "blacklist", "whitelist"], var.cloudfront_geo_restriction_type)
    error_message = "cloudfront_geo_restriction_type must be one of: none, blacklist, whitelist"
  }
}

variable "cloudfront_geo_restriction_list" {
  description = "List of country codes to include for the CloudFront geo restriction"
  type        = list(string)
  default     = []
}

variable "cloudfront_price_class" {
  description = "Price class to use for CloudFront (requires domain to be set)"
  type        = string
  default     = "PriceClass_100"
}

variable "cloudfront_web_acl_id" {
  description = "Optional WAFv2 web ACL ARN to associate with the CloudFront distribution"
  type        = string
  default     = null
}

variable "cloudfront_signed_enabled" {
  description = "Require CloudFront signed URLs for objects under cloudfront_signed_base_path"
  type        = bool
  default     = false

  validation {
    condition = !var.cloudfront_signed_enabled || (
      var.cloudfront_enabled && length(var.cloudfront_signed_public_keys) > 0
    )
    error_message = "cloudfront_signed_enabled requires cloudfront_enabled and at least one cloudfront_signed_public_keys entry."
  }
}

variable "cloudfront_signed_base_path" {
  description = "Path prefix that requires signed URLs (leading and trailing slash, e.g. '/restricted/')"
  type        = string
  default     = "/restricted/"

  validation {
    condition     = can(regex("^/([A-Za-z0-9_-]+/)+$", var.cloudfront_signed_base_path))
    error_message = "cloudfront_signed_base_path must be one or more path segments of letters, digits, '-' or '_', with leading and trailing '/' (e.g. '/restricted/')."
  }
}

variable "cloudfront_signed_public_keys" {
  description = "Trusted signer public keys (name => public PEM), up to 5 for rotation. Can be set before enabling"
  type        = map(string)
  default     = {}

  validation {
    condition     = length(var.cloudfront_signed_public_keys) <= 5
    error_message = "cloudfront_signed_public_keys allows at most 5 keys."
  }

  validation {
    condition = alltrue([
      for name in keys(var.cloudfront_signed_public_keys) : can(regex("^[A-Za-z0-9_-]{1,64}$", name))
    ])
    error_message = "cloudfront_signed_public_keys names must be 1-64 letters, digits, '-' or '_'."
  }

  validation {
    condition = alltrue([
      for pem in values(var.cloudfront_signed_public_keys) :
      strcontains(pem, "BEGIN PUBLIC KEY") && !strcontains(pem, "PRIVATE KEY")
    ])
    error_message = "cloudfront_signed_public_keys values must be public key PEMs; never supply a private key."
  }

  validation {
    condition     = length(var.cloudfront_signed_public_keys) == 0 || var.cloudfront_enabled
    error_message = "cloudfront_signed_public_keys requires cloudfront_enabled."
  }
}

variable "cloudfront_cors_allowed_origins_by_path" {
  description = "CORS allowed origins per path prefix (e.g. '/open-hls/' => ['https://client.edu']). Paths not listed get no CORS headers"
  type        = map(set(string))
  default     = {}

  validation {
    condition = alltrue([
      for path in keys(var.cloudfront_cors_allowed_origins_by_path) : can(regex("^/([A-Za-z0-9_-]+/)+$", path))
    ])
    error_message = "cloudfront_cors_allowed_origins_by_path keys must be one or more path segments of letters, digits, '-' or '_', with leading and trailing '/' (e.g. '/open-hls/')."
  }

  # No nesting, so CloudFront's first-match behavior order never matters
  validation {
    condition = alltrue(flatten([
      for a in keys(var.cloudfront_cors_allowed_origins_by_path) : [
        for b in keys(var.cloudfront_cors_allowed_origins_by_path) : a == b || !startswith(b, a)
      ]
    ]))
    error_message = "cloudfront_cors_allowed_origins_by_path paths must not be nested inside each other."
  }

  validation {
    condition = alltrue(flatten([
      for origins in values(var.cloudfront_cors_allowed_origins_by_path) : concat([length(origins) > 0], [
        for origin in origins :
        can(regex("^https://[a-z0-9.-]+(:[0-9]+)?$", origin)) || can(regex("^http://(localhost|127\\.0\\.0\\.1):[0-9]+$", origin))
      ])
    ]))
    error_message = "cloudfront_cors_allowed_origins_by_path origins must be non-empty sets of exact origins: https://host[:port], or http://localhost:port / http://127.0.0.1:port for local testing."
  }

  validation {
    condition     = length(var.cloudfront_cors_allowed_origins_by_path) == 0 || var.cloudfront_enabled
    error_message = "cloudfront_cors_allowed_origins_by_path requires cloudfront_enabled."
  }

  # An entry equal to the signed base path supplies the signed behavior's CORS
  validation {
    condition = alltrue([
      for path in keys(var.cloudfront_cors_allowed_origins_by_path) :
      path == var.cloudfront_signed_base_path || !var.cloudfront_signed_enabled || (
        !startswith(path, var.cloudfront_signed_base_path) && !startswith(var.cloudfront_signed_base_path, path)
      )
    ])
    error_message = "cloudfront_cors_allowed_origins_by_path paths must not contain or be nested under cloudfront_signed_base_path."
  }

  validation {
    condition     = var.cloudfront_signed_enabled || !contains(keys(var.cloudfront_cors_allowed_origins_by_path), var.cloudfront_signed_base_path)
    error_message = "A cloudfront_cors_allowed_origins_by_path entry for cloudfront_signed_base_path requires cloudfront_signed_enabled."
  }
}

variable "deploy_functions" {
  description = "Enable to deploy functions"
  type        = bool
  default     = false
}

variable "emails_to_notify" {
  description = "Email addresses that will receive metric alarm notifications"
  type        = list(string)
  default     = []
}

variable "functions" {
  description = "Function configurations"
  type = map(object({
    # required
    bucket = string
    file   = string
    # optionals
    env      = optional(map(string), {})
    memory   = optional(number, 128)
    schedule = optional(string, "cron(0 0 1 1,6 ? *)")
    storage  = optional(number, 512)
    timeout  = optional(number, 30)
    tz       = optional(string, "UTC")
  }))
  default = {}
}

variable "stack" {
  description = "Stack name (prefix for resources)"
  type        = string

  # This is additionally verified in Rust stack.rs
  validation {
    condition     = can(regex("^[a-z][a-z0-9]+-[a-z][a-z0-9]+$", var.stack))
    error_message = "Stack name must be two lowercase alphanumeric parts separated by a hyphen (e.g. 'digipres-dev1')."
  }
}

variable "storage_capacity" {
  description = "Storage capacity limit for usage reports (this is an indicator only, it is not enforced)"
  type        = number
  default     = 0

  validation {
    condition = (
      var.storage_capacity >= 0 &&
      floor(var.storage_capacity) == var.storage_capacity
    )
    error_message = "Storage capacity must be a whole number greater than or equal to 0 bytes"
  }
}

variable "tasks" {
  description = "Scheduled ECS Fargate tasks, keyed by short name"
  type = map(object({
    cpu      = number
    mem      = number
    image    = string
    command  = list(string)
    schedule = string
    enabled  = optional(bool, true)
    environment = optional(list(object({
      name  = string
      value = string
    })), [])
    secrets = optional(list(object({
      name      = string
      valueFrom = string
    })), [])
    policy_statements = optional(list(any), [])
  }))
  default = {}
}
