# Batch Operations Role - used for S3 Batch Operations jobs
resource "aws_iam_role" "batch" {
  name = "${local.stack}-s3-batch-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "batchoperations.s3.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })

  tags = { Name = "${local.stack}-s3-batch-role" }
}

resource "aws_iam_role_policy" "batch" {
  name = "${local.stack}-s3-batch-policy"
  role = aws_iam_role.batch.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        # Source bucket for batch copy jobs may be outside the stack prefix
        Effect = "Allow"
        Action = [
          "s3:GetBucketLocation",
          "s3:ListBucket",
          "s3:PutInventoryConfiguration",
        ]
        Resource = "arn:aws:s3:::*"
      },
      {
        Effect = "Allow"
        Action = [
          "s3:GetObject",
          "s3:GetObjectVersion",
          "s3:RestoreObject",
          "s3:GetObjectAcl",
          "s3:GetObjectTagging",
          "s3:GetObjectVersionAcl",
          "s3:GetObjectVersionTagging",
        ]
        Resource = "arn:aws:s3:::*/*"
      },
      {
        Effect = "Allow"
        Action = [
          "s3:PutObject",
          "s3:PutObjectAcl",
          "s3:PutObjectVersionAcl",
          "s3:PutObjectTagging",
          "s3:PutObjectVersionTagging",
        ]
        Resource = "arn:aws:s3:::${local.stack}*/*"
      }
    ]
  })
}

# Replication Role - used for S3 same/cross-region replication
resource "aws_iam_role" "replication" {
  name = "${local.stack}-s3-replication-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "s3.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })

  tags = { Name = "${local.stack}-s3-replication-role" }
}

resource "aws_iam_role_policy" "replication" {
  name = "${local.stack}-s3-replication-policy"
  role = aws_iam_role.replication.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "s3:GetReplicationConfiguration",
          "s3:ListBucket"
        ]
        Resource = "arn:aws:s3:::${local.stack}*"
      },
      {
        Effect = "Allow"
        Action = [
          "s3:GetObjectVersion",
          "s3:GetObjectVersionAcl",
          "s3:GetObjectVersionTagging"
        ]
        Resource = "arn:aws:s3:::${local.stack}*/*"
      },
      {
        Effect = "Allow"
        Action = [
          "s3:GetObjectVersionTagging",
          "s3:ReplicateObject",
          "s3:ReplicateDelete",
          "s3:ReplicateTags"
        ]
        Resource = "arn:aws:s3:::${local.stack}*-repl/*"
      }
    ]
  })
}
