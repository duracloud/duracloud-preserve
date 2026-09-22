use crate::bucket::RequestError;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_account::{
    error::{DisplayErrorContext, ProvideErrorMetadata, SdkError},
    operation::RequestId,
};
use aws_sdk_s3::types::TransitionStorageClass;
use constants::{SFTPGO_NAMESPACE, USER_ACCESS_KEY_NAMESPACE, USER_SECRET_KEY_NAMESPACE};

/// Load default aws sdk config.
pub async fn load_defaults() -> SdkConfig {
    aws_config::load_defaults(BehaviorVersion::latest()).await
}

/// Preserve service metadata and the source chain when wrapping SDK lookup failures.
fn config_error<E>(context: &str, error: &SdkError<E>) -> RequestError
where
    E: std::error::Error + ProvideErrorMetadata + Send + Sync + 'static,
{
    let code = error.code().unwrap_or("unknown");
    // Include parsed metadata, with a header fallback for unparsed error responses.
    let request_id = error
        .meta()
        .request_id()
        .or_else(|| error.request_id())
        .unwrap_or("unknown");

    RequestError::ConfigError(format!(
        "{context}: code={code}, request_id={request_id}, details={}",
        DisplayErrorContext(error)
    ))
}

/// Get the AWS account ID via STS.
pub async fn get_account_id(client: &aws_sdk_sts::Client) -> Result<String, RequestError> {
    let identity = client
        .get_caller_identity()
        .send()
        .await
        .map_err(|e| config_error("failed to get caller identity", &e))?;

    identity
        .account()
        .map(|s| s.to_string())
        .ok_or_else(|| RequestError::ConfigError("no account ID in caller identity".to_string()))
}

/// Get the AWS account name for the current caller's account.
pub async fn get_account_name(client: &aws_sdk_account::Client) -> Result<String, RequestError> {
    let account = client
        .get_account_information()
        .send()
        .await
        .map_err(|e| config_error("failed to get account information", &e))?;

    account
        .account_name()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            RequestError::ConfigError("no account name in account information".to_string())
        })
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
pub async fn get_role_arn(
    client: &aws_sdk_iam::Client,
    role_name: &str,
) -> Result<String, RequestError> {
    let response = client
        .get_role()
        .role_name(role_name)
        .send()
        .await
        .map_err(|e| config_error(&format!("failed to get role '{}'", role_name), &e))?;

    response
        .role()
        .map(|r| r.arn().to_string())
        .ok_or_else(|| RequestError::ConfigError("role missing ARN".to_string()))
}

/// Get an SSM parameter value.
pub async fn get_parameter(
    client: &aws_sdk_ssm::Client,
    param_name: &str,
) -> Result<String, RequestError> {
    let response = client
        .get_parameter()
        .with_decryption(true)
        .name(param_name)
        .send()
        .await
        .map_err(|e| config_error(&format!("failed to get parameter '{}'", param_name), &e))?;

    response
        .parameter()
        .and_then(|p| p.value())
        .map(|v| v.to_string())
        .ok_or_else(|| RequestError::ConfigError("failed to get parameter value".to_string()))
}

/// Get an SFTPGo parameter.
pub async fn get_sftpgo_parameter(
    ssm: &aws_sdk_ssm::Client,
    property: &str,
) -> Result<String, RequestError> {
    let param_name = format!("{SFTPGO_NAMESPACE}{property}");

    get_parameter(ssm, param_name.as_str()).await
}

/// Get an IAM user's credentials (access and secret key).
pub async fn get_user_credentials(
    ssm: &aws_sdk_ssm::Client,
    user_name: &str,
) -> Result<(String, String), RequestError> {
    let access_key_param_name = format!("{USER_ACCESS_KEY_NAMESPACE}{user_name}");
    let secret_key_param_name = format!("{USER_SECRET_KEY_NAMESPACE}{user_name}");

    tokio::try_join!(
        get_parameter(ssm, &access_key_param_name),
        get_parameter(ssm, &secret_key_param_name),
    )
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
    use aws_sdk_account::config::http::{HttpRequest, HttpResponse};
    use aws_smithy_types::body::SdkBody;
    use test_support::{mock_sdk_config, replay_event_with_content_type, replay_xml_event};

    fn error_config(status: u16, body: &str, content_type: &str, request_id: &str) -> SdkConfig {
        let request = HttpRequest::new(SdkBody::empty());
        let mut response = HttpResponse::new(status.try_into().unwrap(), SdkBody::from(body));
        response
            .headers_mut()
            .insert("content-type", content_type.to_string());
        response
            .headers_mut()
            .insert("x-amzn-requestid", request_id.to_string());
        mock_sdk_config((request, response).into()).0
    }

    #[tokio::test]
    async fn test_get_account_id_preserves_sts_error_details() {
        let body = r#"<ErrorResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">
            <Error><Type>Sender</Type><Code>AccessDenied</Code>
                <Message>Caller identity lookup denied</Message></Error>
            <RequestId>sts-request-id</RequestId>
        </ErrorResponse>"#;
        let sdk_config = error_config(403, body, "text/xml", "sts-request-id");
        let client = aws_sdk_sts::Client::new(&sdk_config);

        let error = get_account_id(&client).await.unwrap_err();

        let RequestError::ConfigError(message) = error else {
            panic!("expected configuration error");
        };
        assert!(message.contains("failed to get caller identity"));
        assert!(message.contains("code=AccessDenied"));
        assert!(message.contains("request_id=sts-request-id"));
        assert!(message.contains("Caller identity lookup denied"));
    }

    #[tokio::test]
    async fn test_get_account_name_preserves_throttling_details() {
        let sdk_config = error_config(
            429,
            r#"{"__type":"TooManyRequestsException","message":"Rate exceeded"}"#,
            "application/x-amz-json-1.1",
            "account-throttled-request-id",
        );
        // Exercise error formatting after the SDK gives up, without retrying the fixture.
        let client_config = aws_sdk_account::config::Builder::from(&sdk_config)
            .retry_config(aws_config::retry::RetryConfig::disabled())
            .build();
        let client = aws_sdk_account::Client::from_conf(client_config);

        let error = get_account_name(&client).await.unwrap_err();

        let RequestError::ConfigError(message) = error else {
            panic!("expected configuration error");
        };
        assert!(message.contains("failed to get account information"));
        assert!(message.contains("code=TooManyRequestsException"));
        assert!(message.contains("request_id=account-throttled-request-id"));
        assert!(message.contains("Rate exceeded"));
    }

    #[test]
    fn test_config_error_preserves_timeout_cause_without_service_metadata() {
        let error: SdkError<
            aws_sdk_account::operation::get_account_information::GetAccountInformationError,
        > = SdkError::timeout_error(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "account operation exceeded its deadline",
        ));

        let error = config_error("failed to get account information", &error);

        let RequestError::ConfigError(message) = error else {
            panic!("expected configuration error");
        };
        assert!(message.contains("code=unknown"));
        assert!(message.contains("request_id=unknown"));
        assert!(message.contains("account operation exceeded its deadline"));
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
        let (sdk_config, _replay) = mock_sdk_config(replay_xml_event(200, body));
        let client = aws_sdk_sts::Client::new(&sdk_config);

        let account_id = get_account_id(&client)
            .await
            .expect("get_account_id should succeed");

        assert_eq!(account_id, "123456789012");
    }

    #[tokio::test]
    async fn test_get_account_name_returns_account_name() {
        let body = r#"{"AccountCreatedDate":"2024-01-01T00:00:00Z","AccountId":"123456789012","AccountName":"Example Owner"}"#;
        let (sdk_config, _replay) = mock_sdk_config(replay_event_with_content_type(
            "https://account.us-east-1.amazonaws.com/",
            200,
            body,
            Some("application/x-amz-json-1.1"),
        ));
        let client = aws_sdk_account::Client::new(&sdk_config);

        let account_name = get_account_name(&client)
            .await
            .expect("get_account_name should succeed");

        assert_eq!(account_name, "Example Owner");
    }

    #[tokio::test]
    async fn test_get_account_name_maps_lookup_failures_to_config_error() {
        let body = r#"{"__type":"AccessDeniedException","message":"not authorized"}"#;
        let sdk_config = error_config(
            403,
            body,
            "application/x-amz-json-1.1",
            "account-denied-request-id",
        );
        let client = aws_sdk_account::Client::new(&sdk_config);

        let err = get_account_name(&client)
            .await
            .expect_err("account lookup should return an error");

        match err {
            RequestError::ConfigError(message) => {
                assert!(message.contains("failed to get account information"));
                assert!(message.contains("code=AccessDeniedException"));
                assert!(message.contains("request_id=account-denied-request-id"));
                assert!(message.contains("not authorized"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
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
        let client = aws_sdk_ssm::Client::new(&sdk_config);

        let value = get_parameter(&client, "test-stack-storage-capacity")
            .await
            .expect("get_parameter should succeed");

        assert_eq!(value, "1000");
    }

    #[tokio::test]
    async fn test_get_parameter_maps_ssm_lookup_failures_to_config_error() {
        let body = r#"{"__type":"ParameterNotFound","message":"Parameter not found"}"#;
        let sdk_config = error_config(400, body, "application/x-amz-json-1.1", "ssm-request-id");
        let client = aws_sdk_ssm::Client::new(&sdk_config);

        let err = get_parameter(&client, "missing-param")
            .await
            .expect_err("missing parameter should return an error");

        match err {
            RequestError::ConfigError(message) => {
                assert!(message.contains("failed to get parameter 'missing-param'"));
                assert!(message.contains("code=ParameterNotFound"));
                assert!(message.contains("request_id=ssm-request-id"));
                assert!(message.contains("Parameter not found"));
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
        let client = aws_sdk_iam::Client::new(&sdk_config);

        let arn = get_role_arn(&client, "test-role")
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
        let sdk_config = error_config(404, body, "text/xml", "req-3");
        let client = aws_sdk_iam::Client::new(&sdk_config);

        let err = get_role_arn(&client, "missing-role")
            .await
            .expect_err("missing role should return an error");

        match err {
            RequestError::ConfigError(message) => {
                assert!(message.contains("failed to get role 'missing-role'"));
                assert!(message.contains("code=NoSuchEntity"));
                assert!(message.contains("request_id=req-3"));
                assert!(message.contains("Role missing-role cannot be found."));
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
