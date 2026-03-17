use crate::bucket::RequestError;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_s3::types::TransitionStorageClass;

/// Load default aws sdk config.
pub async fn load_defaults() -> SdkConfig {
    aws_config::load_defaults(BehaviorVersion::latest()).await
}

/// Get the AWS account ID via STS.
pub async fn get_account_id(config: &SdkConfig) -> Result<String, RequestError> {
    let sts_client = aws_sdk_sts::Client::new(config);
    let identity =
        sts_client.get_caller_identity().send().await.map_err(|e| {
            RequestError::ConfigError(format!("failed to get caller identity: {}", e))
        })?;

    identity
        .account()
        .map(|s| s.to_string())
        .ok_or_else(|| RequestError::ConfigError("no account ID in caller identity".to_string()))
}

/// Extract the region from AWS S3 client configuration.
pub fn get_region(client: &aws_sdk_s3::Client) -> Result<String, RequestError> {
    client
        .config()
        .region()
        .map(|r| r.to_string())
        .ok_or_else(|| RequestError::ConfigError("No region configured for S3 client".to_string()))
}

/// Get an IAM role ARN by name.
/// Returns an error if the role does not exist.
pub async fn get_role_arn(config: &SdkConfig, role_name: &str) -> Result<String, RequestError> {
    let iam_client = aws_sdk_iam::Client::new(config);

    let response = iam_client
        .get_role()
        .role_name(role_name)
        .send()
        .await
        .map_err(|e| {
            RequestError::ConfigError(format!("failed to get role '{}': {}", role_name, e))
        })?;

    response
        .role()
        .map(|r| r.arn().to_string())
        .ok_or_else(|| RequestError::ConfigError("role missing ARN".to_string()))
}

/// Get an SSM parameter value.
pub async fn get_parameter(config: &SdkConfig, param_name: &str) -> Result<String, RequestError> {
    let ssm_client = aws_sdk_ssm::Client::new(config);

    let response = ssm_client
        .get_parameter()
        .with_decryption(true)
        .name(param_name)
        .send()
        .await
        .map_err(|e| {
            RequestError::ConfigError(format!("failed to get parameter '{}': {}", param_name, e))
        })?;

    response
        .parameter()
        .and_then(|p| p.value())
        .map(|v| v.to_string())
        .ok_or_else(|| RequestError::ConfigError("failed to get parameter value".to_string()))
}

/// Parse a TransitionStorageClass from its as_str() representation.
/// Only known variants are accepted.
pub fn parse_storage_class(value: &str) -> Option<TransitionStorageClass> {
    match value {
        "DEEP_ARCHIVE" => Some(TransitionStorageClass::DeepArchive),
        "GLACIER" => Some(TransitionStorageClass::Glacier),
        "GLACIER_IR" => Some(TransitionStorageClass::GlacierIr),
        "INTELLIGENT_TIERING" => Some(TransitionStorageClass::IntelligentTiering),
        "ONEZONE_IA" => Some(TransitionStorageClass::OnezoneIa),
        "STANDARD_IA" => Some(TransitionStorageClass::StandardIa),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::{mock_sdk_config, replay_event_with_content_type, replay_xml_event};

    #[tokio::test]
    async fn test_get_account_id_returns_account_from_sts_identity() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<GetCallerIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">
  <GetCallerIdentityResult>
    <Arn>arn:aws:iam::123456789012:user/test</Arn>
    <UserId>AIDATESTUSER</UserId>
    <Account>123456789012</Account>
  </GetCallerIdentityResult>
  <ResponseMetadata>
    <RequestId>req-1</RequestId>
  </ResponseMetadata>
</GetCallerIdentityResponse>"#;
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(200, body));

        let account_id = get_account_id(&sdk_config)
            .await
            .expect("get_account_id should succeed");

        assert_eq!(account_id, "123456789012");
    }

    #[tokio::test]
    async fn test_get_parameter_returns_value() {
        let body = r#"{"Parameter":{"Name":"test-stack-storage-capacity","Type":"SecureString","Value":"1000"}}"#;
        let (sdk_config, _replay) = mock_sdk_config(replay_event_with_content_type(
            "https://test.s3.amazonaws.com/",
            200,
            body,
            Some("application/x-amz-json-1.1"),
        ));

        let value = get_parameter(&sdk_config, "test-stack-storage-capacity")
            .await
            .expect("get_parameter should succeed");

        assert_eq!(value, "1000");
    }

    #[tokio::test]
    async fn test_get_parameter_maps_ssm_lookup_failures_to_config_error() {
        let body = r#"{"__type":"ParameterNotFound","message":"Parameter not found"}"#;
        let (sdk_config, _replay) = mock_sdk_config(replay_event_with_content_type(
            "https://test.s3.amazonaws.com/",
            400,
            body,
            Some("application/x-amz-json-1.1"),
        ));

        let err = get_parameter(&sdk_config, "missing-param")
            .await
            .expect_err("missing parameter should return an error");

        match err {
            RequestError::ConfigError(message) => {
                assert!(message.contains("failed to get parameter 'missing-param'"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_role_arn_returns_role_arn() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<GetRoleResponse xmlns="https://iam.amazonaws.com/doc/2010-05-08/">
  <GetRoleResult>
    <Role>
      <Path>/</Path>
      <RoleName>test-role</RoleName>
      <RoleId>AROATESTROLEID12345</RoleId>
      <Arn>arn:aws:iam::123456789012:role/test-role</Arn>
      <CreateDate>2024-01-01T00:00:00Z</CreateDate>
      <AssumeRolePolicyDocument>%7B%7D</AssumeRolePolicyDocument>
      <MaxSessionDuration>3600</MaxSessionDuration>
    </Role>
  </GetRoleResult>
  <ResponseMetadata>
    <RequestId>req-2</RequestId>
  </ResponseMetadata>
</GetRoleResponse>"#;
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(200, body));

        let arn = get_role_arn(&sdk_config, "test-role")
            .await
            .expect("get_role_arn should succeed");

        assert_eq!(arn, "arn:aws:iam::123456789012:role/test-role");
    }

    #[tokio::test]
    async fn test_get_role_arn_maps_iam_lookup_failures_to_config_error() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ErrorResponse xmlns="https://iam.amazonaws.com/doc/2010-05-08/">
  <Error>
    <Type>Sender</Type>
    <Code>NoSuchEntity</Code>
    <Message>Role missing-role cannot be found.</Message>
  </Error>
  <RequestId>req-3</RequestId>
</ErrorResponse>"#;
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(404, body));

        let err = get_role_arn(&sdk_config, "missing-role")
            .await
            .expect_err("missing role should return an error");

        match err {
            RequestError::ConfigError(message) => {
                assert!(message.contains("failed to get role 'missing-role'"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn test_parse_storage_tier_valid() {
        let tier = parse_storage_class("GLACIER_IR").expect("glacier_ir should parse");
        assert_eq!(tier, TransitionStorageClass::GlacierIr);
    }

    #[test]
    fn test_parse_storage_tier_not_found() {
        let not_found = parse_storage_class("NOT_A_TIER");
        assert!(not_found.is_none());
    }
}
