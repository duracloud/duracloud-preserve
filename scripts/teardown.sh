#!/bin/bash
# scripts/teardown.sh - Script to delete stack resources

STACK=$1

if [ -z "$STACK" ]; then
    echo "Usage: $0 <stack>"
    exit 1
fi

BUCKETS=(
    "bucket-request"
    "managed"
    "private"
    "private-replication"
    "public"
    "public-replication"
)

for bucket in "${BUCKETS[@]}"; do
    ./scripts/buckets.sh empty "${STACK}-${bucket}"
    ./scripts/buckets.sh delete "${STACK}-${bucket}"
done
