use aws_sdk_s3::{
    Client,
    error::{ProvideErrorMetadata, SdkError},
    operation::head_bucket::HeadBucketError,
};
pub use base::bucket::{Bucket, Type};
use constants::BUCKET_TAG_TYPE_KEY;

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
/// Returns `Ok(false)` when the bucket is absent (404/NotFound); other failures
/// (network, permissions, throttling) surface as `Err`.
pub async fn exists(client: &Client, bucket: &str) -> Result<bool, RequestError> {
    match client.head_bucket().bucket(bucket).send().await {
        Ok(_) => Ok(true),
        Err(e) if is_missing_bucket(&e) => Ok(false),
        Err(e) => Err(e).s3_err(format!("failed to head bucket {bucket}")),
    }
}

fn is_missing_bucket(e: &SdkError<HeadBucketError>) -> bool {
    match e {
        SdkError::ServiceError(service) => {
            matches!(service.err(), HeadBucketError::NotFound(_))
                || service
                    .err()
                    .code()
                    .is_some_and(|code| matches!(code, "NotFound" | "NoSuchBucket"))
        }
        _ => false,
    }
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
    match type_from_tags(tags) {
        Some(t) => Ok(Some(Bucket::new(name, t)?)),
        None => Ok(None),
    }
}

/// Look up the AWS region a bucket lives in.
/// Works regardless of the client's configured region.
pub async fn region(client: &Client, bucket: &str) -> Result<String, RequestError> {
    let response = client
        .get_bucket_location()
        .bucket(bucket)
        .send()
        .await
        .s3_err(format!("failed to get location for {bucket}"))?;

    // us-east-1 returns an empty constraint; legacy "EU" represents eu-west-1.
    let region = match response.location_constraint().map(|c| c.as_str()) {
        Some("") | None => "us-east-1",
        Some("EU") => "eu-west-1",
        Some(other) => other,
    };

    Ok(region.to_string())
}

/// Extract the bucket type from an S3 tag set.
pub fn type_from_tags(tags: &[aws_sdk_s3::types::Tag]) -> Option<Type> {
    tags.iter()
        .find(|tag| tag.key() == BUCKET_TAG_TYPE_KEY)
        .and_then(|tag| Type::from_tag_value(tag.value()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::TestClientBuilder;

    #[tokio::test]
    async fn test_exists_returns_true_for_head_success() {
        let client = TestClientBuilder::new().ok().build();

        assert!(exists(&client, "test-bucket").await.unwrap());
    }

    #[tokio::test]
    async fn test_exists_returns_false_for_not_found() {
        let client = TestClientBuilder::new()
            .error(404, "NotFound", "not found")
            .build();

        assert!(!exists(&client, "test-bucket").await.unwrap());
    }

    #[tokio::test]
    async fn test_exists_returns_false_for_no_such_bucket() {
        let client = TestClientBuilder::new()
            .s3_error("NoSuchBucket", "bucket not found")
            .build();

        assert!(!exists(&client, "test-bucket").await.unwrap());
    }

    #[tokio::test]
    async fn test_exists_errors_for_access_denied() {
        let client = TestClientBuilder::new()
            .s3_error("AccessDenied", "forbidden")
            .build();

        let err = exists(&client, "test-bucket")
            .await
            .expect_err("access denied should not be treated as missing");
        assert!(err.to_string().contains("failed to head bucket"));
    }

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
