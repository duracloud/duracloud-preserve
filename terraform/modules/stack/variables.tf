variable "deploy_functions" {
  description = "Enable to deploy functions"
  type        = bool
  default     = false
}

variable "functions" {
  description = "Function configurations"
  type = map(object({
    bucket   = string
    file     = string
    memory   = optional(number, 128)
    timeout  = optional(number, 30)
    schedule = optional(string)
  }))
  default = {}
}

variable "stack" {
  description = "Stack name (prefix for resources)"
  type        = string
}
