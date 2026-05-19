use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::put_object_tagging::PutObjectTaggingError;
use aws_sdk_s3::types::{Tag, Tagging};
use chrono::{DateTime, Utc};

use crate::errors::ArchiveItError;

#[derive(Debug, Clone)]
pub struct ExpirationPolicy {
    pub older_than: DateTime<Utc>,
    /// When set, tag matching S3 objects with this key/value pair.
    pub tag: Option<(String, String)>,
}

pub fn build_tagging(policy: Option<&ExpirationPolicy>) -> Result<Option<Tagging>, ArchiveItError> {
    let Some(policy) = policy else {
        return Ok(None);
    };
    let Some((k, v)) = policy.tag.as_ref() else {
        return Ok(None);
    };
    let tag = Tag::builder()
        .key(k)
        .value(v)
        .build()
        .map_err(|e| ArchiveItError::Io(std::io::Error::other(e.to_string())))?;
    let tagging = Tagging::builder()
        .tag_set(tag)
        .build()
        .map_err(|e| ArchiveItError::Io(std::io::Error::other(e.to_string())))?;
    Ok(Some(tagging))
}

pub async fn tag_expired(
    s3: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    tagging: Tagging,
) -> Result<(), SdkError<PutObjectTaggingError>> {
    s3.put_object_tagging()
        .bucket(bucket)
        .key(key)
        .tagging(tagging)
        .send()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tagging_returns_none_when_policy_absent() {
        let tagging = build_tagging(None).unwrap();
        assert!(tagging.is_none());
    }

    #[test]
    fn build_tagging_returns_none_when_tag_absent() {
        let policy = ExpirationPolicy {
            older_than: Utc::now(),
            tag: None,
        };
        let tagging = build_tagging(Some(&policy)).unwrap();
        assert!(tagging.is_none());
    }

    #[test]
    fn build_tagging_builds_when_tag_present() {
        let policy = ExpirationPolicy {
            older_than: Utc::now(),
            tag: Some(("expired".into(), "true".into())),
        };
        let tagging = build_tagging(Some(&policy)).unwrap().expect("tagging");
        let tags = tagging.tag_set();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].key(), "expired");
        assert_eq!(tags[0].value(), "true");
    }
}
