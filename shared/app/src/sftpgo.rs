use std::time::Duration;

use ::sftpgo::{
    BaseVirtualFolder, Error, FolderUpsert, SFTPGoClient, SFTPGoConfig, base_folders, permissions,
    virtual_folders,
};
use futures::{StreamExt, TryStreamExt, stream};
use reqwest::StatusCode;
use tokio::time::{Instant, sleep, timeout_at};

const MAX_FOLDER_CONCURRENCY: usize = 8;
const MAX_FOLDER_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(250);
const FOLDER_RETRY_BUDGET: Duration = Duration::from_secs(15);

/// Build a token-authenticated SFTPGo client ready for API calls.
pub async fn connect(host: &str, username: &str, password: &str) -> Result<SFTPGoClient, Error> {
    let mut client = SFTPGoClient::new(
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()?,
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

    stream::iter(base_folders(
        &user_key, buckets, region, access_key, secret_key,
    ))
    .map(|folder| async move {
        let outcome = upsert_folder_with_retry(client, &folder).await?;
        tracing::debug!(folder = %folder.name, ?outcome, "SFTPGo folder synced");
        Ok::<_, Error>(())
    })
    .buffer_unordered(MAX_FOLDER_CONCURRENCY)
    .try_collect::<()>()
    .await?;

    user.permissions = permissions(buckets);
    user.virtual_folders = virtual_folders(&user_key, buckets);
    client.update_user(&user).await?;

    Ok(())
}

/// Retry the whole upsert so a lost create response is followed by a fresh existence check.
/// The budget includes requests and backoff; retries retain their concurrency slot.
async fn upsert_folder_with_retry(
    client: &SFTPGoClient,
    folder: &BaseVirtualFolder,
) -> Result<FolderUpsert, Error> {
    let deadline = Instant::now() + FOLDER_RETRY_BUDGET;
    let mut attempt = 1;
    loop {
        let result = timeout_at(deadline, client.upsert_folder(folder))
            .await
            .map_err(|_| Error::FolderTimeout {
                folder: folder.name.clone(),
            })?;
        match result {
            Err(error) if attempt < MAX_FOLDER_ATTEMPTS && is_transient(&error) => {
                let backoff = RETRY_BASE_DELAY * 2u32.pow(attempt - 1);
                let jitter =
                    Duration::from_millis(rand::random_range(0..=backoff.as_millis() as u64));
                let retry_after = match &error {
                    Error::Api { retry_after, .. } => retry_after.unwrap_or_default(),
                    _ => Duration::ZERO,
                };
                let delay = retry_after.saturating_add(jitter);
                // Never shorten the server's requested delay to fit our budget.
                if delay >= deadline.saturating_duration_since(Instant::now()) {
                    return Err(error);
                }
                tracing::warn!(folder = %folder.name, attempt, delay_ms = delay.as_millis() as u64,
                    "Retrying transient SFTPGo folder failure");
                sleep(delay).await;
                attempt += 1;
            }
            result => return result,
        }
    }
}

fn is_transient(error: &Error) -> bool {
    match error {
        Error::Request(error) => error.is_connect() || error.is_timeout(),
        Error::Api { status, .. } => {
            *status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
        }
        Error::FolderTimeout { .. } => false,
    }
}
