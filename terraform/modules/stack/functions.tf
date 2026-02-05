locals {
  deploy_functions = var.deploy_functions

  functions = {
    bucket-request    = {}
    checksum-report   = {}
    compute-checksums = {}
    inventory-report  = {}
    storage-report    = {}
  }
}
