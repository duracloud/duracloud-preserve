variable "deploy_functions" {
  description = "Enable to deploy functions"
  type        = bool
  default     = false
}

variable "function_bucket" {
  description = "S3 bucket that contains function zip files"
  type        = string
}

variable "function_files" {
  description = "Function file names"
  type        = map(string)
  default     = {}
}

variable "function_prefix" {
  description = "S3 prefix within bucket that contains function zip files"
  default     = "/"
  type        = string
}

variable "stack" {
  description = "Stack name (prefix for resources)"
  type        = string
}
