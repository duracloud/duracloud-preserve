//! Integration tests for bucket_request::perform
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test --test bucket_request -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "int-test")
//!   - Run: make setup s=<stack> p=<profile>

mod common;

use apputils::content_type;
use aws_smithy_types::body::SdkBody;
use awsutils::bucket::{delete, empty, exists};
use awsutils::file::File;
use awsutils::test_client::integration_test_config;
use common::timestamp;

#[tokio::test]
#[ignore]
async fn test_perform() {
    let config = integration_test_config().await;
    let ts = timestamp();
    let bucket_name = format!("perf-{}", ts);

    let primary = format!("{}-{}", config.stack().as_str(), bucket_name);
    let repl = format!("{}-{}-repl", config.stack().as_str(), bucket_name);

    // Note: we upload to the managed bucket (not the request bucket) intentionally
    // because if the function is deployed we don't want to process twice
    let file = File::new(
        config.stack().managed_bucket(),
        format!("bucket-request/test-{}.txt", ts),
    );
    awsutils::file::upload(
        config.s3(),
        &file,
        SdkBody::from(bucket_name.as_bytes()),
        content_type::TEXT_PLAIN,
    )
    .await
    .unwrap();

    let opts = awsutils::bucket_request::PerformOptions::default();
    awsutils::bucket_request::perform(&config, &file, &opts)
        .await
        .unwrap();

    assert!(exists(config.s3(), &primary).await);
    assert!(exists(config.s3(), &repl).await);

    assert!(
        !awsutils::file::exists(config.s3(), &file).await,
        "request file should be deleted after processing"
    );

    empty(config.s3(), &primary).await.unwrap();
    empty(config.s3(), &repl).await.unwrap();
    delete(config.s3(), &primary).await.unwrap();
    delete(config.s3(), &repl).await.unwrap();
}
