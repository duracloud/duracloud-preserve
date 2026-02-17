use crate::bucket::RequestError;
use aws_config::{BehaviorVersion, SdkConfig};

/// Load default aws sdk config.
pub async fn default_config() -> SdkConfig {
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

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_runtime::client::http::test_util::{ReplayEvent, StaticReplayClient};
    use aws_smithy_types::body::SdkBody;

    fn replay_event(status: u16, body: &str) -> ReplayEvent {
        ReplayEvent::new(
            http::Request::builder()
                .uri("https://test.amazonaws.com/")
                .body(SdkBody::empty())
                .expect("request should build"),
            http::Response::builder()
                .status(status)
                .header("Content-Type", "text/xml")
                .body(SdkBody::from(body.to_string()))
                .expect("response should build"),
        )
    }

    fn mock_sdk_config(event: ReplayEvent) -> (SdkConfig, StaticReplayClient) {
        let http_client = StaticReplayClient::new(vec![event]);
        let sdk_config = SdkConfig::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(aws_config::Region::new("us-east-1"))
            .credentials_provider(aws_sdk_s3::config::SharedCredentialsProvider::new(
                aws_sdk_s3::config::Credentials::for_tests(),
            ))
            .http_client(http_client.clone())
            .build();

        (sdk_config, http_client)
    }

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
        let (sdk_config, _replay) = mock_sdk_config(replay_event(200, body));

        let account_id = get_account_id(&sdk_config)
            .await
            .expect("get_account_id should succeed");

        assert_eq!(account_id, "123456789012");
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
        let (sdk_config, _replay) = mock_sdk_config(replay_event(200, body));

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
        let (sdk_config, _replay) = mock_sdk_config(replay_event(404, body));

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
}
