variable "cert_ready" {
  description = "Set to true after the ACM certificate has been validated (if using domain)"
  type        = bool
  default     = false
}

variable "deploy_functions" {
  description = "Enable to deploy functions"
  type        = bool
  default     = false
}

variable "domain" {
  description = "The domain that will be used for public file access"
  type        = string
  default     = ""
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

variable "name" {
  description = "Friendly name to asscociate with this stack (i.e. Lyrasis)"
  type        = string
}

variable "stack" {
  description = "Stack name (prefix for resources)"
  type        = string

  # This is additionally verified in Rust stack.rs
  validation {
    condition     = can(regex("^[a-z][a-z0-9]+-[a-z][a-z0-9]+$", var.stack))
    error_message = "Stack name must be two lowercase alphanumeric parts separated by a hyphen (e.g. 'digipress-dev1')."
  }
}
