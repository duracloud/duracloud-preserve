variable "bucket" {
  description = "Bucket to create"
  type        = string
}

variable "functions" {
  description = "Function configurations (used to extract files to upload)"
  type = map(object({
    bucket   = string
    file     = string
    memory   = optional(number)
    schedule = optional(string)
  }))
  default = {}
}
