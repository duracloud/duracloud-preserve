# Storage report policy, permissions and notifications
resource "aws_ssm_parameter" "storage_capacity" {
  name  = local.storage_capacity_param_name
  type  = "String"
  value = local.storage_capacity
}
