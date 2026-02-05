variable "bucket" {
  description = "Bucket to create"
  type        = string
}

variable "files" {
  description = "Files to upload (map of name to path)"
  type        = map(string)
  default     = {}
}
