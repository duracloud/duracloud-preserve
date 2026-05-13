variable "archive_it_enabled" {
  description = "Enable Archive-It scheduled tasks (inventory, audit, sync)"
  type        = bool
  default     = false
}

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

variable "cloudfront_enabled" {
  description = "Enable CloudFront distribution for public file access"
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
