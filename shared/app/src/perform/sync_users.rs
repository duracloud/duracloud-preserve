use std::collections::HashMap;

use aws_config::SdkConfig;
use awsutils::{
    bucket::Bucket, config::get_user_credentials, users::list_users_with_email_and_groups,
};
use base::{Stack, bucket::Type};

use crate::{bucket, errors::SyncUsersError};

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub username: Option<String>,
    pub sftpgo_host: String,
    pub sftpgo_username: String,
    pub sftpgo_password: String,
}

pub async fn perform(config: &SdkConfig, _args: &PerformArgs) -> Result<(), SyncUsersError> {
    let iam = aws_sdk_iam::Client::new(config);
    let s3 = aws_sdk_s3::Client::new(config);
    let ssm = aws_sdk_ssm::Client::new(config);
    let mut bucket_cache: HashMap<String, Vec<Bucket>> = HashMap::new();

    // TODO filter users by username if supplied (becomes list of 1 if found or empty if not)
    let users = list_users_with_email_and_groups(&iam).await?;

    if users.is_empty() {
        tracing::error!("No eligible users were found");
        return Err(SyncUsersError::UserDiscovery);
    }

    for user in users {
        tracing::info!("Processing user: {:?}", user.email);

        let (access_key, _secret_key) =
            get_user_credentials(&ssm, &user.user_name)
                .await
                .map_err(|source| SyncUsersError::UserCredentials {
                    user_name: user.user_name.clone(),
                    source,
                })?;

        tracing::info!("Retrieved access key: {}", access_key);

        // TODO: filter out empty stack name candidates
        let stacks = user
            .groups
            .iter()
            .map(|group| group.rsplitn(3, '-').last().unwrap_or("").to_string())
            .collect::<Vec<String>>();

        let mut buckets = Vec::new();
        for stack_name in &stacks {
            let stack_buckets = match bucket_cache.get(stack_name) {
                Some(cached) => cached.clone(),
                None => {
                    let stack =
                        Stack::new(stack_name).map_err(|reason| SyncUsersError::InvalidStack {
                            stack: stack_name.clone(),
                            reason: reason.to_string(),
                        })?;
                    let fetched = bucket::list_for_stack_by_type(
                        &s3,
                        &stack,
                        &[Type::Internal, Type::Public, Type::Standard],
                    )
                    .await?;
                    bucket_cache.insert(stack_name.clone(), fetched.clone());
                    fetched
                }
            };
            buckets.extend(stack_buckets);
        }

        let buckets = buckets
            .iter()
            .map(|bucket| bucket.name())
            .collect::<Vec<&str>>();

        tracing::info!("Identified buckets: {}", buckets.join(","));
    }

    Ok(())
}
