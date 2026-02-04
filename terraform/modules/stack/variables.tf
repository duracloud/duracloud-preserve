variable "deploy_functions" {
  description = "Enable to deploy functions"
  type        = bool
  default     = false
}

variable "stack" {
  description = "Stack name (prefix for resources)"
  type        = string
}
