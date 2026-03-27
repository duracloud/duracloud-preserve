pub use apputils::bucket::{
    BUCKET_TAG_STACK_KEY, BUCKET_TAG_TYPE_KEY, Bucket, BucketPair, Name, REPLICATION_SUFFIX, Type,
    make_pairs, review_request_names, to_primary, to_replication,
};
use aws_sdk_s3::Client;

pub use crate::errors::RequestError;
use crate::errors::S3ResultExt;

/// Delete an empty bucket.
pub async fn delete(client: &Client, bucket: &str) -> Result<(), RequestError> {
    client
        .delete_bucket()
        .bucket(bucket)
        .send()
        .await
        .s3_err(format!("failed to delete bucket {bucket}"))?;

    Ok(())
}

/// Empty all objects from a bucket (handles versioned objects).
pub async fn empty(client: &Client, bucket: &str) -> Result<(), RequestError> {
    use aws_sdk_s3::types::{Delete, ObjectIdentifier};

    loop {
        let response = client
            .list_object_versions()
            .bucket(bucket)
            .max_keys(1000)
            .send()
            .await
            .s3_err("failed to list versions")?;

        let mut objects_to_delete = Vec::new();

        for version in response.versions() {
            if let (Some(key), Some(version_id)) = (version.key(), version.version_id()) {
                objects_to_delete.push(
                    ObjectIdentifier::builder()
                        .key(key)
                        .version_id(version_id)
                        .build()
                        .s3_err("failed to build object id")?,
                );
            }
        }

        for marker in response.delete_markers() {
            if let (Some(key), Some(version_id)) = (marker.key(), marker.version_id()) {
                objects_to_delete.push(
                    ObjectIdentifier::builder()
                        .key(key)
                        .version_id(version_id)
                        .build()
                        .s3_err("failed to build object id")?,
                );
            }
        }

        if objects_to_delete.is_empty() {
            break;
        }

        client
            .delete_objects()
            .bucket(bucket)
            .delete(
                Delete::builder()
                    .set_objects(Some(objects_to_delete))
                    .build()
                    .s3_err("failed to build delete")?,
            )
            .send()
            .await
            .s3_err("failed to delete objects")?;
    }

    Ok(())
}

/// Check if a bucket exists.
pub async fn exists(client: &Client, bucket: &str) -> bool {
    client.head_bucket().bucket(bucket).send().await.is_ok()
}

/// Fetch bucket from S3 and determine its type from tags.
/// Returns `Ok(None)` if bucket doesn't exist or has no valid BucketType tag.
pub async fn from_name(client: &Client, name: &str) -> Result<Option<Bucket>, RequestError> {
    let Ok(response) = client.get_bucket_tagging().bucket(name).send().await else {
        return Ok(None);
    };

    from_tags(name, response.tag_set())
}

/// Construct bucket from an S3 tag set.
/// Returns `None` if no valid BucketType tag is found.
pub fn from_tags(
    name: &str,
    tags: &[aws_sdk_s3::types::Tag],
) -> Result<Option<Bucket>, RequestError> {
    let bucket_type = tags
        .iter()
        .find(|tag| tag.key() == BUCKET_TAG_TYPE_KEY)
        .and_then(|tag| Type::from_tag_value(tag.value()));

    match bucket_type {
        Some(t) => Ok(Some(Bucket::new(name, t)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_tags() {
        use aws_sdk_s3::types::Tag;

        let tags_with_type = vec![
            Tag::builder().key("Other").value("value").build().unwrap(),
            Tag::builder()
                .key(BUCKET_TAG_TYPE_KEY)
                .value("standard")
                .build()
                .unwrap(),
        ];
        let result = from_tags("test-bucket", &tags_with_type).unwrap();
        assert!(result.is_some());
        let bucket = result.unwrap();
        assert_eq!(bucket.name(), "test-bucket");
        assert_eq!(bucket.bucket_type(), &Type::Standard);

        let tags_without_type = vec![Tag::builder().key("Other").value("value").build().unwrap()];
        let result = from_tags("test-bucket", &tags_without_type).unwrap();
        assert!(result.is_none());

        let empty_tags: Vec<Tag> = vec![];
        let result = from_tags("test-bucket", &empty_tags).unwrap();
        assert!(result.is_none());
    }
}
