/// Return the `AllowPublicRead` policy JSON for a public bucket.
pub fn public_read_policy(bucket_name: &str) -> String {
    serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Sid": "AllowPublicRead",
            "Effect": "Allow",
            "Principal": "*",
            "Action": "s3:GetObject",
            "Resource": format!("arn:aws:s3:::{}/*", bucket_name)
        }]
    })
    .to_string()
}

/// Return the `DenyAllUploads` policy JSON used as a temporary guard during bucket setup.
pub fn deny_uploads_policy(bucket_name: &str) -> String {
    serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Sid": "DenyAllUploads",
            "Effect": "Deny",
            "Principal": "*",
            "Action": "s3:PutObject",
            "Resource": format!("arn:aws:s3:::{}/*", bucket_name)
        }]
    })
    .to_string()
}

/// Check whether a policy JSON string matches the expected `AllowPublicRead` shape.
pub fn is_public_read_policy(policy_json: &str, bucket_name: &str) -> Result<bool, String> {
    let v: serde_json::Value = serde_json::from_str(policy_json)
        .map_err(|e| format!("failed to parse bucket policy: {e}"))?;

    let expected_resource = format!("arn:aws:s3:::{}/*", bucket_name);

    let ok = v
        .get("Statement")
        .and_then(|s| s.as_array())
        .is_some_and(|statements| {
            statements.iter().any(|stmt| {
                stmt.get("Sid").and_then(|s| s.as_str()) == Some("AllowPublicRead")
                    && stmt.get("Effect").and_then(|e| e.as_str()) == Some("Allow")
                    && stmt.get("Principal").and_then(|p| p.as_str()) == Some("*")
                    && stmt.get("Action").and_then(|a| a.as_str()) == Some("s3:GetObject")
                    && stmt.get("Resource").and_then(|r| r.as_str())
                        == Some(expected_resource.as_str())
            })
        });

    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_read_policy_round_trips() {
        let policy = public_read_policy("my-bucket");
        assert!(is_public_read_policy(&policy, "my-bucket").unwrap());
    }

    #[test]
    fn public_read_policy_wrong_bucket() {
        let policy = public_read_policy("my-bucket");
        assert!(!is_public_read_policy(&policy, "other-bucket").unwrap());
    }

    #[test]
    fn is_public_read_rejects_deny_policy() {
        let policy = deny_uploads_policy("my-bucket");
        assert!(!is_public_read_policy(&policy, "my-bucket").unwrap());
    }

    #[test]
    fn is_public_read_rejects_garbage() {
        assert!(is_public_read_policy("not json", "b").is_err());
    }
}
