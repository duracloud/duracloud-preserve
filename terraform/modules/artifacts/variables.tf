variable "bucket" {
  description = "Bucket to create"
  type        = string
}

variable "files" {
  description = "Files to upload (map of name to path)"
  type        = map(string)
  default     = {}
}

variable "org_id" {
  description = "AWS Organization ID (e.g. o-abcd1234) allowed to read artifacts"
  type        = string
}
