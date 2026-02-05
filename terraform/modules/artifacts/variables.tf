variable "bucket" {
  description = "Bucket to create"
  type        = string
}

variable "files" {
  description = "Files to upload"
  type        = map(string)
  default     = {}
}
