use awsutils::{config::get_user_credentials, users::list_users_with_email_and_groups};

use crate::{
    bucket::{self, BucketCache},
    config::Clients,
    errors::SyncUsersError,
    sftpgo,
};

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub username: Option<String>,
    pub sftpgo_host: String,
    pub sftpgo_username: String,
    pub sftpgo_password: String,
}

pub async fn perform(clients: &Clients, args: &PerformArgs) -> Result<(), SyncUsersError> {
    let client = sftpgo::connect(
        &args.sftpgo_host,
        &args.sftpgo_username,
        &args.sftpgo_password,
    )
    .await?;
    let region = awsutils::config::get_region(&clients.s3)?;
    let mut bucket_cache = BucketCache::new();

    // TODO filter users by username if supplied (becomes list of 1 if found or empty if not)
    let users = list_users_with_email_and_groups(&clients.iam).await?;

    if users.is_empty() {
        tracing::error!("No eligible users were found");
        return Err(SyncUsersError::UserDiscovery);
    }

    for user in users {
        tracing::info!("Processing user: {:?}", user.email);

        let buckets = bucket::list_for_user_stacks(&clients.s3, &user, &mut bucket_cache).await?;
        let bucket_names: Vec<&str> = buckets.iter().map(|b| b.name()).collect();

        if bucket_names.is_empty() {
            tracing::warn!("No buckets found for user");
            continue;
        }

        tracing::info!("Identified buckets: {}", bucket_names.join(","));

        let (access_key, secret_key) = get_user_credentials(&clients.ssm, &user.user_name)
            .await
            .map_err(|source| SyncUsersError::UserCredentials {
                user_name: user.user_name.clone(),
                source,
            })?;

        sftpgo::sync_user_access(
            &client,
            &user.email,
            &bucket_names,
            &region,
            &access_key,
            &secret_key,
        )
        .await?;

        tracing::info!("User account updated successfully")
    }

    Ok(())
}
