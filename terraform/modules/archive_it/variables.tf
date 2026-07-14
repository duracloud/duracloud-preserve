variable "stack" {
  description = "Stack name (prefix for resources)"
  type        = string
}

variable "expiration" {
  description = "When set, audit reports (and tags) Archive-It WARCs older than N years as expired"
  type        = number
  default     = null

  validation {
    condition     = var.expiration == null || var.expiration > 0
    error_message = "expiration must be greater than 0 when set."
  }
}

variable "image" {
  description = "dcp image for all tasks (per-task config.image overrides)"
  type        = string
  default     = "duracloud/dcp:latest"
}

variable "config" {
  description = "Per-task overrides keyed by short name: audit, inventory, sync"
  type = map(object({
    cpu      = optional(number)
    mem      = optional(number)
    image    = optional(string)
    schedule = optional(string)
    enabled  = optional(bool)
  }))
  default = {}

  validation {
    condition     = alltrue([for k in keys(var.config) : contains(["audit", "inventory", "sync"], k)])
    error_message = "config keys must be one of: audit, inventory, sync."
  }
}
