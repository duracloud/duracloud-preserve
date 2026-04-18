use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("{endpoint} returned {status}: {body}")]
    Api {
        endpoint: String,
        status: StatusCode,
        body: String,
    },
}

pub struct SFTPGoClient {
    client: Client,
    config: SFTPGoConfig,
    token: String,
}

#[derive(Debug)]
pub struct SFTPGoConfig {
    pub host: String,
    pub username: String,
    pub password: String,
}

impl SFTPGoClient {
    pub fn new(client: Client, config: SFTPGoConfig) -> Self {
        Self {
            client,
            config,
            token: String::new(),
        }
    }

    pub async fn get_token(&mut self) -> Result<(), Error> {
        let resp = self
            .client
            .get(format!("{}/api/v2/token", self.config.host))
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await?;

        let token_resp: TokenResponse = resp.json().await?;
        self.token = token_resp.access_token;
        Ok(())
    }

    pub async fn get_user(&self, username: &str) -> Result<User, Error> {
        Ok(self
            .client
            .get(format!("{}/api/v2/users/{}", self.config.host, username))
            .bearer_auth(&self.token)
            .send()
            .await?
            .json()
            .await?)
    }

    pub async fn get_users(&self) -> Result<Vec<User>, Error> {
        Ok(self
            .client
            .get(format!("{}/api/v2/users", self.config.host))
            .bearer_auth(&self.token)
            .send()
            .await?
            .json()
            .await?)
    }

    pub async fn update_user(&self, user: &User) -> Result<(), Error> {
        let resp = self
            .client
            .put(format!(
                "{}/api/v2/users/{}",
                self.config.host, user.username
            ))
            .bearer_auth(&self.token)
            .json(user)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            return Err(Error::Api {
                endpoint: format!("update_user {}", user.username),
                status,
                body: resp.text().await.unwrap_or_default(),
            });
        }
        Ok(())
    }

    pub async fn upsert_folder(&self, folder: &BaseVirtualFolder) -> Result<(), Error> {
        let folder_url = format!("{}/api/v2/folders/{}", self.config.host, folder.name);
        let probe = self
            .client
            .get(&folder_url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        let exists = match probe.status() {
            s if s.is_success() => true,
            StatusCode::NOT_FOUND => false,
            status => {
                return Err(Error::Api {
                    endpoint: format!("probe folder {}", folder.name),
                    status,
                    body: probe.text().await.unwrap_or_default(),
                });
            }
        };

        let resp = if exists {
            self.client.put(&folder_url)
        } else {
            self.client
                .post(format!("{}/api/v2/folders", self.config.host))
        }
        .bearer_auth(&self.token)
        .json(folder)
        .send()
        .await?;

        let status = resp.status();
        if !status.is_success() {
            return Err(Error::Api {
                endpoint: format!("upsert_folder {}", folder.name),
                status,
                body: resp.text().await.unwrap_or_default(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct User {
    pub username: String,
    #[serde(default)]
    pub permissions: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub virtual_folders: Vec<VirtualFolder>,
    #[serde(default)]
    last_login: i64,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl User {
    /// Sanitize [`User::username`] into a filesystem-safe identifier by
    /// replacing every non-alphanumeric character with `_`.
    ///
    /// `test.user@example.com` → `test_user_example_com`.
    pub fn key(&self) -> String {
        self.username
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    }

    pub fn last_login_date(&self) -> Option<DateTime<Utc>> {
        if self.last_login == 0 {
            None
        } else {
            DateTime::from_timestamp_millis(self.last_login)
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct VirtualFolder {
    pub name: String,
    pub virtual_path: String,
    #[serde(default)]
    pub quota_size: i64,
    #[serde(default)]
    pub quota_files: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<FilesystemConfig>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BaseVirtualFolder {
    pub name: String,
    pub mapped_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<FilesystemConfig>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct FilesystemConfig {
    pub provider: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3config: Option<S3Config>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl FilesystemConfig {
    pub const PROVIDER_S3: i32 = 1;

    pub fn s3(config: S3Config) -> Self {
        Self {
            provider: Self::PROVIDER_S3,
            s3config: Some(config),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub access_secret: Secret,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Secret {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl Secret {
    pub fn plain(payload: impl Into<String>) -> Self {
        Self {
            status: "Plain".to_string(),
            payload: Some(payload.into()),
            extra: Map::new(),
        }
    }
}

/// Build the S3-backed folder definitions to upsert via
/// [`SFTPGoClient::upsert_folder`] before calling
/// [`SFTPGoClient::update_user`].
///
/// Names come from [`folder_name`]: `{user_key}-{bucket}`.
///
/// ```no_run
/// use sftpgo::base_folders;
///
/// let folders = base_folders(
///     "user1",
///     &["acme-managed", "acme-private"],
///     "us-east-1",
///     "AKIA...",
///     "secret...",
/// );
/// # let _ = folders;
/// ```
pub fn base_folders(
    user_key: &str,
    buckets: &[&str],
    region: &str,
    access_key: &str,
    access_secret: &str,
) -> Vec<BaseVirtualFolder> {
    buckets
        .iter()
        .map(|bucket| {
            let name = folder_name(user_key, bucket);
            BaseVirtualFolder {
                mapped_path: format!("/tmp/sftpgo-folders/{name}"),
                name,
                filesystem: Some(FilesystemConfig::s3(S3Config {
                    bucket: (*bucket).into(),
                    region: region.into(),
                    access_key: access_key.into(),
                    access_secret: Secret::plain(access_secret),
                    ..Default::default()
                })),
                extra: Map::new(),
            }
        })
        .collect()
}

/// Build the SFTPGo folder name for a `(user_key, bucket)` pair.
///
/// Each user gets their own folder record (with their own
/// credentials) even when multiple users reference the same underlying
/// bucket, because folder names in SFTPGo are globally unique.
pub fn folder_name(user_key: &str, bucket: &str) -> String {
    format!("{user_key}-{bucket}")
}

/// Build a permissions map for a set of bucket-backed virtual paths.
///
/// `/` is always `["list"]`. Buckets whose name ends with `-managed`
/// get `["list", "download"]`; everything else gets `["*"]`.
///
/// ```no_run
/// use sftpgo::permissions;
///
/// let perms = permissions(&["acme-managed", "acme-private"]);
/// // { "/":             ["list"],
/// //   "/acme-managed": ["list", "download"],
/// //   "/acme-private": ["*"] }
/// # let _ = perms;
/// ```
pub fn permissions(buckets: &[&str]) -> HashMap<String, Vec<String>> {
    let mut perms = HashMap::from([("/".to_string(), vec!["list".to_string()])]);
    for bucket in buckets {
        let perm = if bucket.contains('-') && bucket.ends_with("-managed") {
            vec!["list".to_string(), "download".to_string()]
        } else {
            vec!["*".to_string()]
        };
        perms.insert(format!("/{bucket}"), perm);
    }
    perms
}

/// Build the per-user folder mappings for [`User::virtual_folders`].
///
/// Each mapping references a folder (created via
/// [`base_folders`] + [`SFTPGoClient::upsert_folder`]) by name and
/// mounts it at `/{bucket}` in the user's view.
///
/// ```no_run
/// use sftpgo::virtual_folders;
///
/// let mappings = virtual_folders("user1", &["acme-managed", "acme-private"]);
/// # let _ = mappings;
/// ```
pub fn virtual_folders(user_key: &str, buckets: &[&str]) -> Vec<VirtualFolder> {
    buckets
        .iter()
        .map(|bucket| VirtualFolder {
            name: folder_name(user_key, bucket),
            virtual_path: format!("/{bucket}"),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_folders_s3_filesystem_is_populated() {
        let folders = base_folders("user1", &["acme-managed"], "us-east-1", "AKIA", "sekret");
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "user1-acme-managed");
        assert_eq!(
            folders[0].mapped_path,
            "/tmp/sftpgo-folders/user1-acme-managed"
        );
        let fs = folders[0].filesystem.as_ref().unwrap();
        assert_eq!(fs.provider, FilesystemConfig::PROVIDER_S3);
        let s3 = fs.s3config.as_ref().unwrap();
        assert_eq!(s3.bucket, "acme-managed");
        assert_eq!(s3.region, "us-east-1");
        assert_eq!(s3.access_key, "AKIA");
        assert_eq!(s3.access_secret.status, "Plain");
        assert_eq!(s3.access_secret.payload.as_deref(), Some("sekret"));
    }

    #[test]
    fn folder_name_combines_user_key_and_bucket() {
        assert_eq!(folder_name("user1", "acme-managed"), "user1-acme-managed");
    }

    #[test]
    fn permissions_bare_managed_is_not_treated_as_managed() {
        let perms = permissions(&["managed"]);
        assert_eq!(perms.get("/managed"), Some(&vec!["*".to_string()]));
    }

    #[test]
    fn permissions_managed_suffix_gets_list_and_download() {
        let perms = permissions(&["acme-managed"]);
        assert_eq!(
            perms.get("/acme-managed"),
            Some(&vec!["list".to_string(), "download".to_string()]),
        );
    }

    #[test]
    fn permissions_non_managed_gets_wildcard() {
        let perms = permissions(&["acme-private", "acme-public"]);
        assert_eq!(perms.get("/acme-private"), Some(&vec!["*".to_string()]));
        assert_eq!(perms.get("/acme-public"), Some(&vec!["*".to_string()]));
    }

    #[test]
    fn permissions_root_is_list_only() {
        let perms = permissions(&[]);
        assert_eq!(perms.get("/"), Some(&vec!["list".to_string()]));
        assert_eq!(perms.len(), 1);
    }

    #[test]
    fn user_key_sanitizes_email_to_dir_name() {
        let user = User {
            username: "test.user@example.com".to_string(),
            ..Default::default()
        };
        assert_eq!(user.key(), "test_user_example_com");
    }

    #[test]
    fn virtual_folders_name_and_path_scheme() {
        let folders = virtual_folders("user1", &["acme-managed", "acme-private"]);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].name, "user1-acme-managed");
        assert_eq!(folders[0].virtual_path, "/acme-managed");
        assert!(folders[0].filesystem.is_none());
        assert_eq!(folders[1].name, "user1-acme-private");
        assert_eq!(folders[1].virtual_path, "/acme-private");
    }
}
