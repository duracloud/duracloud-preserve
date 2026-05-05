# Batch Operations Role - used for S3 Batch Operations jobs
locals {
  # names derived from generated constants (_locals.tf)
  batch_role_name         = "${local.stack}${local.batch_role_suffix}"
  batch_policy_name       = "${local.stack}${local.batch_policy_suffix}"
  replication_role_name   = "${local.stack}${local.replication_role_suffix}"
  replication_policy_name = "${local.stack}${local.replication_policy_suffix}"
}

data "aws_iam_policy_document" "batch_assume_role" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["batchoperations.s3.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "batch" {
  name               = local.batch_role_name
  assume_role_policy = data.aws_iam_policy_document.batch_assume_role.json

  tags = { Name = local.batch_role_name }
}

data "aws_iam_policy_document" "batch" {
  statement {
    # Source bucket for batch copy jobs may be outside the stack prefix
    effect = "Allow"
    actions = [
      "s3:GetBucketLocation",
      "s3:ListBucket",
      "s3:PutInventoryConfiguration",
    ]
    resources = ["arn:aws:s3:::*"]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:GetObject",
      "s3:GetObjectVersion",
      "s3:RestoreObject",
      "s3:GetObjectAcl",
      "s3:GetObjectTagging",
      "s3:GetObjectVersionAcl",
      "s3:GetObjectVersionTagging",
    ]
    resources = ["arn:aws:s3:::*/*"]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:PutObject",
      "s3:PutObjectAcl",
      "s3:PutObjectVersionAcl",
      "s3:PutObjectTagging",
      "s3:PutObjectVersionTagging",
    ]
    resources = [local.stack_object_arn_pattern]
  }
}

resource "aws_iam_role_policy" "batch" {
  name   = local.batch_policy_name
  role   = aws_iam_role.batch.id
  policy = data.aws_iam_policy_document.batch.json
}

# Replication Role - used for S3 same/cross-region replication
data "aws_iam_policy_document" "replication_assume_role" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["s3.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "replication" {
  name               = local.replication_role_name
  assume_role_policy = data.aws_iam_policy_document.replication_assume_role.json

  tags = { Name = local.replication_role_name }
}

data "aws_iam_policy_document" "replication" {
  statement {
    effect = "Allow"
    actions = [
      "s3:GetReplicationConfiguration",
      "s3:ListBucket",
    ]
    resources = [local.stack_bucket_arn_pattern]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:GetObjectVersion",
      "s3:GetObjectVersionAcl",
      "s3:GetObjectVersionTagging",
    ]
    resources = [local.stack_object_arn_pattern]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:GetObjectVersionTagging",
      "s3:ReplicateObject",
      "s3:ReplicateDelete",
      "s3:ReplicateTags",
    ]
    resources = [local.repl_object_arn_pattern]
  }
}

resource "aws_iam_role_policy" "replication" {
  name   = local.replication_policy_name
  role   = aws_iam_role.replication.id
  policy = data.aws_iam_policy_document.replication.json
}
