#!/bin/bash
# scripts/create-replication-role.sh - Create S3 replication IAM role for a stack

STACK=$1

if [ -z "$STACK" ]; then
    echo "Usage: $0 <stack>"
    exit 1
fi

ROLE_NAME="${STACK}-s3-replication-role"
POLICY_NAME="${STACK}-s3-replication-policy"

# Check if role already exists
if aws iam get-role --role-name "$ROLE_NAME" > /dev/null 2>&1; then
    echo "Role $ROLE_NAME already exists"
    aws iam get-role --role-name "$ROLE_NAME" --query 'Role.Arn' --output text
    exit 0
fi

echo "Creating role $ROLE_NAME..."

# Create the assume role policy document
ASSUME_ROLE_POLICY=$(cat <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Service": "s3.amazonaws.com"
            },
            "Action": "sts:AssumeRole"
        }
    ]
}
EOF
)

# Create the role
aws iam create-role \
    --role-name "$ROLE_NAME" \
    --assume-role-policy-document "$ASSUME_ROLE_POLICY" \
    --tags Key=Name,Value="$ROLE_NAME"

# Create the replication policy document
REPLICATION_POLICY=$(cat <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "s3:GetReplicationConfiguration",
                "s3:ListBucket"
            ],
            "Resource": "arn:aws:s3:::${STACK}*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "s3:GetObjectVersion",
                "s3:GetObjectVersionAcl",
                "s3:GetObjectVersionTagging"
            ],
            "Resource": "arn:aws:s3:::${STACK}*/*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "s3:GetObjectVersionTagging",
                "s3:ReplicateObject",
                "s3:ReplicateDelete",
                "s3:ReplicateTags"
            ],
            "Resource": "arn:aws:s3:::${STACK}*-repl/*"
        }
    ]
}
EOF
)

# Attach the inline policy
aws iam put-role-policy \
    --role-name "$ROLE_NAME" \
    --policy-name "$POLICY_NAME" \
    --policy-document "$REPLICATION_POLICY"

echo "Role created successfully:"
aws iam get-role --role-name "$ROLE_NAME" --query 'Role.Arn' --output text
