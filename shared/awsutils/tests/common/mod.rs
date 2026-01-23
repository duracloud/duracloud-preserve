#![allow(dead_code)]

use awsutils::{
    bucket::{delete, empty},
    config::RequestConfig,
};

pub async fn cleanup_bucket(config: &RequestConfig, bucket: &str) {
    let _ = empty(&config.client, bucket).await;
    let _ = delete(&config.client, bucket).await;
}

pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
