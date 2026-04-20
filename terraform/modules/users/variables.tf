variable "sftpgo_enabled" {
  description = "Enable SFTPGo integration (creates users in SFTPGo)"
  default     = false
  type        = bool
}
variable "users" {
  description = "Create users for stack"
  default     = {}
  type = map(object({
    email   = string
    enabled = optional(bool, true)
    buckets = optional(list(string), [])
    memberships = list(object({
      stack = string
      group = string
    }))
  }))
  validation {
    condition = alltrue([
      for u in var.users : alltrue([
        for m in u.memberships : contains(["power-users", "restricted-users", "standard-users"], m.group)
      ])
    ])
    error_message = "Group must be 'power-users', 'restricted-users' or 'standard-users'."
  }
}
