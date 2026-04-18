resource "sftpgo_user" "user" {
  for_each = local.sftpgo_host != null ? local.users : {}

  username = each.value.email
  password = each.key
  email    = each.value.email
  home_dir = "/srv/sftpgo/data/${replace(replace(lower(each.value.email), "/[^a-z0-9_-]/", "_"), "/_+/", "_")}"
  status   = each.value.enabled ? 1 : 0

  filesystem = {
    provider = 0
  }

  permissions = {
    "/" = "list"
  }

  filters = {
    external_auth_disabled  = true
    require_password_change = true

    web_client = [
      "shares-disabled"
    ]
  }

  lifecycle {
    ignore_changes = [
      password,
      permissions,
      virtual_folders,
      filters.check_password_disabled,
      filters.require_password_change
    ]
  }
}
