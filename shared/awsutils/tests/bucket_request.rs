//! Integration tests for bucket_request::perform
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test --test bucket_request -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "inttest")
//!   - Run: make setup s=<stack> p=<profile>

mod common;

use apputils::content_type;
use aws_smithy_types::body::SdkBody;
use awsutils::bucket::{delete, empty, exists};
use awsutils::config::test_config;
use awsutils::file::File;
use common::timestamp;

#[tokio::test]
#[ignore]
async fn test_perform() {
    let config = test_config().await;
    let ts = timestamp();
    let bucket_name = format!("perf-{}", ts);

    let primary = format!("{}-{}", config.stack().as_str(), bucket_name);
    let repl = format!("{}-{}-repl", config.stack().as_str(), bucket_name);

    let file = File::new(config.stack().request_bucket(), format!("test-{}.txt", ts));
    awsutils::file::upload(
        &config.client,
        &file,
        SdkBody::from(bucket_name.as_bytes()),
        content_type::TEXT_PLAIN,
    )
    .await
    .unwrap();

    awsutils::bucket_request::perform(&config, &file)
        .await
        .unwrap();

    assert!(exists(&config.client, &primary).await);
    assert!(exists(&config.client, &repl).await);

    // TODO: verify result file uploaded

    assert!(
        !awsutils::file::exists(&config.client, &file).await,
        "request file should be deleted after processing"
    );

    empty(&config.client, &primary).await.unwrap();
    empty(&config.client, &repl).await.unwrap();
    delete(&config.client, &primary).await.unwrap();
    delete(&config.client, &repl).await.unwrap();
}

// TODO: with failure
