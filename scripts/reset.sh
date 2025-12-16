#!/bin/bash
# scripts/reset.sh - Script to reset stack resources

ACTION=$1
STACK=$2

BUCKETS=(
    "bucket-request"
    "managed"
    "private"
    "private-repl"
    "public"
    "public-repl"
)

if [ -z "$ACTION" ]; then
    echo "Usage: $0 <action> <stack>"
    echo "Actions: empty, delete"
    exit 1
fi

if [ -z "$STACK" ]; then
    echo "Error: Stack name is required for $ACTION action"
    echo "Usage: $0 $ACTION <stack>"
    exit 1
fi

case $ACTION in
    empty)
        echo "Emptying buckets for stack: $STACK"
        for bucket in "${BUCKETS[@]}"; do
            echo "Emptying ${STACK}-${bucket}..."
            ./scripts/bucket.sh empty "${STACK}-${bucket}" || true
        done
    ;;
    delete)
        echo "Deleting buckets for stack: $STACK"
        for bucket in "${BUCKETS[@]}"; do
            echo "Deleting ${STACK}-${bucket}..."
            ./scripts/bucket.sh delete "${STACK}-${bucket}" ||true
        done
    ;;
    *)
        echo "Invalid action. Use 'empty' or 'delete'."
        exit 1
    ;;
esac
