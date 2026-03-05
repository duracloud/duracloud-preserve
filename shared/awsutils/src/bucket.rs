use apputils::bucket::{
    BUCKET_REQUEST_CONTENT_TYPE, MAX_BUCKETS_PER_REQUEST, MAX_REQUEST_FILE_SIZE,
};

pub use apputils::bucket::{
    BUCKET_TAG_STACK_KEY, BUCKET_TAG_TYPE_KEY, Bucket, BucketPair, Name, REPLICATION_SUFFIX, Type,
    pair_buckets, primary_bucket, replication_bucket, review_bucket_names,
};
use aws_sdk_s3::Client;
use tokio::io::AsyncBufReadExt;

pub use crate::errors::RequestError;
use crate::file::{File, download};

/// Delete an empty bucket.
pub async fn delete(client: &Client, bucket: &str) -> Result<(), RequestError> {
    client
        .delete_bucket()
        .bucket(bucket)
        .send()
        .await
        .map_err(|e| RequestError::S3Error(format!("failed to delete bucket {}: {}", bucket, e)))?;

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
            .map_err(|e| RequestError::S3Error(format!("failed to list versions: {}", e)))?;

        let mut objects_to_delete = Vec::new();

        for version in response.versions() {
            if let (Some(key), Some(version_id)) = (version.key(), version.version_id()) {
                objects_to_delete.push(
                    ObjectIdentifier::builder()
                        .key(key)
                        .version_id(version_id)
                        .build()
                        .map_err(|e| {
                            RequestError::S3Error(format!("failed to build object id: {}", e))
                        })?,
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
                        .map_err(|e| {
                            RequestError::S3Error(format!("failed to build object id: {}", e))
                        })?,
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
                    .map_err(|e| RequestError::S3Error(format!("failed to build delete: {}", e)))?,
            )
            .send()
            .await
            .map_err(|e| RequestError::S3Error(format!("failed to delete objects: {}", e)))?;
    }

    Ok(())
}

/// Check if a bucket exists.
pub async fn exists(client: &Client, bucket: &str) -> bool {
    client.head_bucket().bucket(bucket).send().await.is_ok()
}

/// Retrieve bucket request file and verify it is valid.
pub async fn get_bucket_names(client: &Client, file: &File) -> Result<Vec<String>, RequestError> {
    let Ok(r) = download(client, file).await else {
        return Err(RequestError::S3Error("failed to download file".to_string()));
    };

    if let Some(ct) = r.content_type()
        && !ct.starts_with(BUCKET_REQUEST_CONTENT_TYPE)
    {
        return Err(RequestError::InvalidContentType);
    }

    if let Some(len) = r.content_length()
        && len > MAX_REQUEST_FILE_SIZE as i64
    {
        return Err(RequestError::FileTooLarge {
            actual: len,
            max: MAX_REQUEST_FILE_SIZE as i64,
        });
    }

    let reader = r.body.into_async_read();
    let mut names = Vec::new();
    let mut buf_reader = tokio::io::BufReader::new(reader).lines();

    while let Ok(Some(line)) = buf_reader.next_line().await {
        names.push(line);
        if names.len() >= MAX_BUCKETS_PER_REQUEST as usize {
            break;
        }
    }

    Ok(names)
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
    use test_support::TestClientBuilder;

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

    #[tokio::test]
    async fn test_get_bucket_names() {
        let content = "123\n456\n789\n234\n567\n890";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain".to_string()))
            .build();

        let names = get_bucket_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), 5);
        assert_eq!(names[0], "123");
        assert_eq!(names[1], "456");
        assert_eq!(names[2], "789");
        assert_eq!(names[3], "234");
    }

    #[tokio::test]
    async fn test_get_bucket_names_content_type_with_charset() {
        let content = "bucket1\nbucket2";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain; charset=utf-8".to_string()))
            .build();

        let names = get_bucket_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "bucket1");
        assert_eq!(names[1], "bucket2");
    }

    #[tokio::test]
    async fn test_get_bucket_names_exceeds_size_limit() {
        let content = "a".repeat((MAX_REQUEST_FILE_SIZE + 1) as usize);
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain".to_string()))
            .build();

        let result = get_bucket_names(&client, &file).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::FileTooLarge { actual, max }) => {
                assert_eq!(actual, (MAX_REQUEST_FILE_SIZE + 1) as i64);
                assert_eq!(max, MAX_REQUEST_FILE_SIZE as i64);
            }
            _ => panic!("Expected FileTooLarge error"),
        }
    }

    #[tokio::test]
    async fn test_get_bucket_names_invalid_content_type() {
        let content = "bucket1\nbucket2";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("application/json".to_string()))
            .build();

        let result = get_bucket_names(&client, &file).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(RequestError::InvalidContentType)));
    }

    #[tokio::test]
    async fn test_get_bucket_names_truncates_at_max() {
        let content = "a\nb\nc\nd\ne\nf\ng";
        let file = File::new("test-bucket", "buckets.txt");
        let client = TestClientBuilder::new()
            .success(content, Some("text/plain".to_string()))
            .build();

        let names = get_bucket_names(&client, &file).await.unwrap();

        assert_eq!(names.len(), MAX_BUCKETS_PER_REQUEST as usize);
    }
}
