use ::sftpgo::{Error, SFTPGoClient, SFTPGoConfig, base_folders, permissions, virtual_folders};

/// Build a token-authenticated SFTPGo client ready for API calls.
pub async fn connect(host: &str, username: &str, password: &str) -> Result<SFTPGoClient, Error> {
    let mut client = SFTPGoClient::new(
        reqwest::Client::new(),
        SFTPGoConfig {
            host: host.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        },
    );
    client.get_token().await?;
    Ok(client)
}

/// Upsert the user's virtual folders and mount them into their SFTPGo account.
pub async fn sync_user_access(
    client: &SFTPGoClient,
    email: &str,
    buckets: &[&str],
    region: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<(), Error> {
    let mut user = client.get_user(email).await?;
    let user_key = user.key();

    tracing::info!("Found SFTPGo user account: {}", user.username);

    for folder in base_folders(&user_key, buckets, region, access_key, secret_key) {
        client.upsert_folder(&folder).await?;
    }

    user.permissions = permissions(buckets);
    user.virtual_folders = virtual_folders(&user_key, buckets);
    client.update_user(&user).await?;

    Ok(())
}
