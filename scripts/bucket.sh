#!/bin/bash
# scripts/bucket.sh - Script to list, create, empty or delete a bucket

ACTION=$1
BUCKET_NAME=$2

if [ -z "$ACTION" ]; then
    echo "Usage: $0 <action> [bucket-name]"
    echo "Actions: list, create, empty, delete"
    echo "Note: bucket-name is required for create, empty, and delete actions"
    exit 1
fi

if [ "$ACTION" != "list" ] && [ -z "$BUCKET_NAME" ]; then
    echo "Error: Bucket name is required for $ACTION action"
    echo "Usage: $0 $ACTION <bucket-name>"
    exit 1
fi

empty_bucket_versions() {
    aws s3api list-object-versions \
        --bucket "$BUCKET_NAME" \
        --query '{Objects: Versions[].{Key:Key,VersionId:VersionId} || `[]`}' \
        --output json > tmp_objects.json

    if [ -s tmp_objects.json ]; then
        if jq -e '.Objects != null and (.Objects | length) > 0' tmp_objects.json > /dev/null; then
            aws s3api delete-objects \
                --bucket "$BUCKET_NAME" \
                --delete file://tmp_objects.json
        fi
    fi

    aws s3api list-object-versions \
        --bucket "$BUCKET_NAME" \
        --query '{Objects: DeleteMarkers[].{Key:Key,VersionId:VersionId} || `[]`}' \
        --output json > tmp_markers.json

    if [ -s tmp_markers.json ]; then
        if jq -e '.Objects != null and (.Objects | length) > 0' tmp_markers.json > /dev/null; then
            aws s3api delete-objects \
                --bucket "$BUCKET_NAME" \
                --delete file://tmp_markers.json
        fi
    fi

    rm -f tmp_objects.json tmp_markers.json
}

case $ACTION in
    list)
        aws s3api list-buckets
    ;;
    create)
        aws s3 mb s3://$BUCKET_NAME
    ;;
    empty)
        empty_bucket_versions
        aws s3 rm s3://$BUCKET_NAME --recursive
    ;;
    delete)
        aws s3 rb s3://$BUCKET_NAME --force
    ;;
    *)
        echo "Invalid action. Use 'list', 'create', 'empty', or 'delete'."
        exit 1
    ;;
esac
