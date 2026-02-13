#!/bin/bash
# scripts/trigger-compute-checksums.sh - Invoke compute-checksums Lambda with sample scheduled event payload

set -euo pipefail

STACK="${1:-}"
PROFILE="${2:-}"

if [ -z "$STACK" ] || [ -z "$PROFILE" ]; then
    echo "Usage: $0 <stack> <aws-profile>"
    exit 1
fi

mkdir -p payloads
sed "s/my-stack/$STACK/g" functions/compute-checksums/events/sample.json > payloads/compute-checksums.json

AWS_PROFILE="$PROFILE" aws lambda invoke \
    --function-name "${STACK}-compute-checksums" \
    --payload fileb://payloads/compute-checksums.json \
    --cli-binary-format raw-in-base64-out \
    payloads/compute-checksums.response.json > payloads/compute-checksums.invoke.json

if [ "$(cat payloads/compute-checksums.response.json)" != "null" ]; then
    echo "Function payload:"
    cat payloads/compute-checksums.response.json
    echo ""
fi

echo "Invoke metadata:"
cat payloads/compute-checksums.invoke.json
