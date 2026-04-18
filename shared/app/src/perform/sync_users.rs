use std::collections::HashMap;

use awsutils::{
    bucket::Bucket, config::get_user_credentials, users::list_users_with_email_and_groups,
};
use base::{Stack, bucket::Type};

use crate::{bucket, config::Clients, errors::SyncUsersError};

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub username: Option<String>,
    pub sftpgo_host: String,
    pub sftpgo_username: String,
    pub sftpgo_password: String,
}

pub async fn perform(clients: &Clients, _args: &PerformArgs) -> Result<(), SyncUsersError> {
    let mut bucket_cache: HashMap<String, Vec<Bucket>> = HashMap::new();

    // TODO filter users by username if supplied (becomes list of 1 if found or empty if not)
    let users = list_users_with_email_and_groups(&clients.iam).await?;

    if users.is_empty() {
        tracing::error!("No eligible users were found");
        return Err(SyncUsersError::UserDiscovery);
    }

    for user in users {
        tracing::info!("Processing user: {:?}", user.email);

        let (access_key, _secret_key) =
            match get_user_credentials(&clients.ssm, &user.user_name).await {
                Ok(credentials) => credentials,
                Err(source) => {
                    return Err(SyncUsersError::UserCredentials {
                        user_name: user.user_name.clone(),
                        source,
                    });
                }
            };

        tracing::info!("Retrieved access key: {}", access_key);

        let stacks: Vec<Stack> = user
            .groups
            .iter()
            .filter_map(|group| match Stack::from_prefixed_name(group) {
                Ok(stack) => Some(stack),
                Err(reason) => {
                    tracing::debug!("Skipping group '{group}' (not a stack name): {reason}");
                    None
                }
            })
            .collect();

        let mut buckets = Vec::new();
        for stack in &stacks {
            let stack_buckets = match bucket_cache.get(stack.as_str()) {
                Some(cached) => cached.clone(),
                None => {
                    let fetched = bucket::list_for_stack_by_type(
                        &clients.s3,
                        stack,
                        &[Type::Internal, Type::Public, Type::Standard],
                    )
                    .await?;
                    bucket_cache.insert(stack.as_str().to_string(), fetched.clone());
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
