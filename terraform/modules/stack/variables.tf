variable "deploy_functions" {
  description = "Enable to deploy functions"
  type        = bool
  default     = false
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
    schedule = optional(string)
    storage  = optional(number, 512)
    timeout  = optional(number, 30)
  }))
  default = {}
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
