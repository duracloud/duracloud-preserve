#!/bin/bash
# scripts/trigger-function.sh - Invoke a Lambda function with its sample scheduled event payload

set -euo pipefail

FUNCTION="${1:-}"
STACK="${2:-}"
PROFILE="${3:-}"

if [ -z "$FUNCTION" ] || [ -z "$STACK" ] || [ -z "$PROFILE" ]; then
  echo "Usage: $0 <function> <stack> <aws-profile>"
  exit 1
fi

SAMPLE="functions/$FUNCTION/events/sample.json"

if [ ! -f "$SAMPLE" ]; then
  echo "Error: $SAMPLE not found"
  exit 1
fi

mkdir -p payloads
sed "s/my-stack/$STACK/g" "$SAMPLE" >"payloads/$FUNCTION.json"

AWS_PROFILE="$PROFILE" aws lambda invoke \
  --function-name "${STACK}-${FUNCTION}" \
  --payload "fileb://payloads/$FUNCTION.json" \
  --cli-binary-format raw-in-base64-out \
  "payloads/$FUNCTION.response.json" >"payloads/$FUNCTION.invoke.json"

if [ "$(cat "payloads/$FUNCTION.response.json")" != "null" ]; then
  echo "Function payload:"
  cat "payloads/$FUNCTION.response.json"
  echo ""
fi

echo "Invoke metadata:"
cat "payloads/$FUNCTION.invoke.json"
