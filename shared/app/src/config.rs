use aws_config::{Region, SdkConfig, retry::RetryConfig};
use base::Stack;
use std::time::Duration;

use awsutils::{
    bucket::{self, RequestError},
    config as aws_config_utils,
};

/// Cost Explorer is a global service with a single endpoint in us-east-1.
const COST_EXPLORER_REGION: &str = "us-east-1";

/// AWS SDK clients
pub struct Clients {
    pub account: aws_sdk_account::Client,
    pub cost_explorer: aws_sdk_costexplorer::Client,
    pub iam: aws_sdk_iam::Client,
    pub s3: aws_sdk_s3::Client,
    pub s3control: aws_sdk_s3control::Client,
    pub ssm: aws_sdk_ssm::Client,
    pub sts: aws_sdk_sts::Client,
}

impl Clients {
    pub fn new(sdk_config: &SdkConfig) -> Self {
        let account_timeout = sdk_config
            .timeout_config()
            .map(|config| config.to_builder())
            .unwrap_or_default()
            .operation_timeout(Duration::from_secs(15))
            .build();
        let account_config = sdk_config
            .to_builder()
            .retry_config(RetryConfig::standard().with_max_attempts(5))
            .timeout_config(account_timeout)
            .build();

        let cost_explorer_config = sdk_config
            .to_builder()
            .region(Region::new(COST_EXPLORER_REGION))
            .build();

        let s3_config = sdk_config
            .to_builder()
            .retry_config(RetryConfig::adaptive().with_max_attempts(5))
            .build();

        Self {
            account: aws_sdk_account::Client::new(&account_config),
            cost_explorer: aws_sdk_costexplorer::Client::new(&cost_explorer_config),
            iam: aws_sdk_iam::Client::new(sdk_config),
            s3: aws_sdk_s3::Client::new(&s3_config),
            s3control: aws_sdk_s3control::Client::new(sdk_config),
            ssm: aws_sdk_ssm::Client::new(sdk_config),
            sts: aws_sdk_sts::Client::new(sdk_config),
        }
    }
}

/// Common configuration for all functions
pub struct Config {
    account_id: String,
    clients: Clients,
    debug_handler: bool,
    roles: Roles,
    stack: Stack,
    storage_capacity: u64,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("account_id", &self.account_id)
            .field("debug_handler", &self.debug_handler)
            .field("roles", &self.roles)
            .field("stack", &self.stack)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// Create a Config for tests from a mocked SDK config.
    pub fn for_tests(sdk_config: SdkConfig, debug_handler: bool) -> Self {
        Self {
            account_id: "123456789".to_string(),
            clients: Clients::new(&sdk_config),
            debug_handler,
            roles: Roles {
                batch: "arn:aws:iam::123456789:role/test-batch-role".to_string(),
                replication: "arn:aws:iam::123456789:role/test-replication-role".to_string(),
            },
            stack: Stack::new("test-stack").expect("test stack should be valid"),
            storage_capacity: 0,
        }
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn batch_role_arn(&self) -> &str {
        &self.roles.batch
    }

    pub fn clients(&self) -> &Clients {
        &self.clients
    }

    pub fn debug_handler(&self) -> bool {
        self.debug_handler
    }

    pub fn replication_role_arn(&self) -> &str {
        &self.roles.replication
    }

    pub fn s3(&self) -> &aws_sdk_s3::Client {
        &self.clients.s3
    }

    pub fn s3control(&self) -> &aws_sdk_s3control::Client {
        &self.clients.s3control
    }

    pub fn stack(&self) -> &Stack {
        &self.stack
    }

    pub fn storage_capacity(&self) -> u64 {
        self.storage_capacity
    }
}

/// Role ARNs for the stack.
#[derive(Debug, Clone)]
pub struct Roles {
    pub batch: String,
    pub replication: String,
}

/// Create a Config for the stack.
pub async fn load(stack: Stack) -> Result<Config, RequestError> {
    let sdk_config = aws_config_utils::load_defaults().await;
    load_with_sdk_config(stack, sdk_config).await
}

async fn load_with_sdk_config(stack: Stack, sdk_config: SdkConfig) -> Result<Config, RequestError> {
    let managed_bucket = stack.managed_bucket();
    let clients = Clients::new(&sdk_config);

    if !bucket::exists(&clients.s3, &managed_bucket).await? {
        return Err(RequestError::ConfigError(format!(
            "failed to find managed bucket for stack (does this stack exist?): {}",
            managed_bucket
        )));
    }

    let account_id = aws_config_utils::get_account_id(&clients.sts).await?;
    let batch_role_name = stack.batch_role_name();
    let storage_capacity_param_name = stack.storage_capacity_param_name();
    let replication_role_name = stack.replication_role_name();

    let (batch_role, replication_role, storage_capacity) = tokio::try_join!(
        aws_config_utils::get_role_arn(&clients.iam, &batch_role_name),
        aws_config_utils::get_role_arn(&clients.iam, &replication_role_name),
        aws_config_utils::get_parameter(&clients.ssm, &storage_capacity_param_name),
    )?;

    let roles = Roles {
        batch: batch_role,
        replication: replication_role,
    };

    let storage_capacity = storage_capacity.parse::<u64>().map_err(|e| {
        RequestError::ValidationError(format!("failed to parse storage capacity: {}", e))
    })?;

    Ok(Config {
        account_id,
        clients,
        debug_handler: false,
        roles,
        stack,
        storage_capacity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_http_client::test_util::NeverClient;
    use test_support::{TestClientBuilder, recorded_requests, replay_event_with_content_type};

    fn account_error(status: u16, code: &str) -> aws_smithy_http_client::test_util::ReplayEvent {
        replay_event_with_content_type(
            "https://account.us-east-1.amazonaws.com/",
            status,
            format!(r#"{{"__type":"{code}","message":"test service error"}}"#),
            Some("application/x-amz-json-1.1"),
        )
    }

    #[tokio::test(start_paused = true)]
    async fn test_account_lookup_recovers_from_throttling_and_transient_errors() {
        let (sdk_config, replay) = TestClientBuilder::new()
            .event(account_error(429, "TooManyRequestsException"))
            .event(account_error(500, "InternalServerException"))
            .success(
                r#"{"AccountName":"Example Owner"}"#,
                Some("application/x-amz-json-1.1".to_string()),
            )
            .build_sdk_config_with_replay();
        let clients = Clients::new(&sdk_config);

        let owner = aws_config_utils::get_account_name(&clients.account)
            .await
            .expect("account lookup should recover from retryable errors");

        assert_eq!(owner, "Example Owner");
        assert_eq!(recorded_requests(&replay).len(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn test_account_lookup_stops_after_five_attempts() {
        let mut builder = TestClientBuilder::new();
        for _ in 0..5 {
            builder = builder.event(account_error(429, "TooManyRequestsException"));
        }
        let (sdk_config, replay) = builder.build_sdk_config_with_replay();
        let clients = Clients::new(&sdk_config);

        let error = clients
            .account
            .get_account_information()
            .send()
            .await
            .unwrap_err();

        assert!(
            error
                .as_service_error()
                .unwrap()
                .is_too_many_requests_exception()
        );
        assert_eq!(recorded_requests(&replay).len(), 5);
    }

    #[tokio::test(start_paused = true)]
    async fn test_account_lookup_does_not_retry_access_denied() {
        let (sdk_config, replay) = TestClientBuilder::new()
            .event(account_error(403, "AccessDeniedException"))
            .build_sdk_config_with_replay();
        let clients = Clients::new(&sdk_config);

        let error = clients
            .account
            .get_account_information()
            .send()
            .await
            .unwrap_err();

        assert!(
            error
                .as_service_error()
                .unwrap()
                .is_access_denied_exception()
        );
        assert_eq!(recorded_requests(&replay).len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_account_lookup_times_out_after_fifteen_seconds() {
        let http_client = NeverClient::new();
        let sdk_config = TestClientBuilder::new()
            .build_sdk_config()
            .to_builder()
            .http_client(http_client.clone())
            .build();
        let clients = Clients::new(&sdk_config);
        let started = tokio::time::Instant::now();

        let error = clients
            .account
            .get_account_information()
            .send()
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            aws_sdk_account::error::SdkError::TimeoutError(_)
        ));
        assert_eq!(started.elapsed(), Duration::from_secs(15));
        assert_eq!(http_client.num_calls(), 1);
    }

    #[tokio::test]
    async fn test_load_does_not_request_account_information() {
        let identity = r#"<GetCallerIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">
            <GetCallerIdentityResult><Account>123456789012</Account></GetCallerIdentityResult>
        </GetCallerIdentityResponse>"#;
        let role = |name: &str| {
            format!(
                r#"<GetRoleResponse xmlns="https://iam.amazonaws.com/doc/2010-05-08/">
                    <GetRoleResult><Role>
                        <Path>/</Path><RoleName>{name}</RoleName><RoleId>AROATEST</RoleId>
                        <Arn>arn:aws:iam::123456789012:role/{name}</Arn>
                        <CreateDate>2024-01-01T00:00:00Z</CreateDate>
                    </Role></GetRoleResult>
                </GetRoleResponse>"#
            )
        };
        // HEAD bucket, STS identity, two IAM roles, and SSM capacity only.
        let (sdk_config, replay) = TestClientBuilder::new()
            .ok()
            .success(identity, Some("text/xml".to_string()))
            .success(role("batch"), Some("text/xml".to_string()))
            .success(role("replication"), Some("text/xml".to_string()))
            .success(
                r#"{"Parameter":{"Value":"1000"}}"#,
                Some("application/x-amz-json-1.1".to_string()),
            )
            .build_sdk_config_with_replay();

        let config = load_with_sdk_config(Stack::new("test-stack").unwrap(), sdk_config)
            .await
            .expect("common configuration should load without account information");

        assert_eq!(config.account_id(), "123456789012");
        assert_eq!(config.storage_capacity(), 1000);
        let requests = recorded_requests(&replay);
        assert_eq!(requests.len(), 5);
        assert!(requests.iter().all(|r| !r.uri.contains("account.")));
    }
}
