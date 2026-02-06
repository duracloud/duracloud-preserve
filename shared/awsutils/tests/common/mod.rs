#![allow(dead_code)]

use awsutils::{
    bucket::{delete, empty},
    config::Config,
};

pub async fn cleanup_bucket(config: &Config, bucket: &str) {
    let _ = empty(config.s3(), bucket).await;
    let _ = delete(config.s3(), bucket).await;
}

pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
