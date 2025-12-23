#!/bin/bash
# scripts/set-managed-bucket-policy.sh - Set bucket policy for managed bucket to accept inventory reports

STACK=$1

if [ -z "$STACK" ]; then
    echo "Usage: $0 <stack>"
    exit 1
fi

MANAGED_BUCKET="${STACK}-managed"

# Get AWS account ID
ACCOUNT_ID=$(aws sts get-caller-identity --query 'Account' --output text)

if [ -z "$ACCOUNT_ID" ]; then
    echo "Error: Failed to get AWS account ID"
    exit 1
fi

echo "Setting bucket policy for ${MANAGED_BUCKET} to accept inventory reports..."

# Create the bucket policy document allowing S3 to write inventory reports
BUCKET_POLICY=$(cat <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Sid": "AllowS3InventoryDelivery",
            "Effect": "Allow",
            "Principal": {
                "Service": "s3.amazonaws.com"
            },
            "Action": "s3:PutObject",
            "Resource": "arn:aws:s3:::${MANAGED_BUCKET}/*",
            "Condition": {
                "StringEquals": {
                    "s3:x-amz-acl": "bucket-owner-full-control",
                    "aws:SourceAccount": "${ACCOUNT_ID}"
                },
                "ArnLike": {
                    "aws:SourceArn": "arn:aws:s3:::${STACK}*"
                }
            }
        }
    ]
}
EOF
)

# Apply the bucket policy
aws s3api put-bucket-policy \
    --bucket "$MANAGED_BUCKET" \
    --policy "$BUCKET_POLICY"

echo "Bucket policy set successfully for ${MANAGED_BUCKET}"
