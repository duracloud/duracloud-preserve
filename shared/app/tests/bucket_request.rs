//! Integration tests for bucket_request::perform.
//!
//! These tests make real AWS calls and should be run with:
//!   cargo test -p app --test bucket_request -- --ignored --test-threads=1
//!
//! Prerequisites:
//!   - Set TEST_STACK env var (defaults to "int-test")
//!   - Run: make setup s=<stack> p=<profile>

use app::{config, perform::bucket_request};
use apputils::content_type;
use aws_smithy_types::body::SdkBody;
use awsutils::bucket::{self};
use awsutils::file::{self, File};

#[tokio::test]
#[ignore]
async fn test_perform() {
    let config = test_support::integration_test_config(config::load).await;
    let ts = test_support::unix_timestamp_secs();
    let bucket_name = format!("perf-{}", ts);

    let primary = format!("{}-{}", config.stack().as_str(), bucket_name);
    let repl = format!("{}-{}-repl", config.stack().as_str(), bucket_name);

    // Upload to the managed bucket (not the request bucket) intentionally so
    // deployed functions do not process the same request twice.
    let file = File::new(
        config.stack().managed_bucket(),
        format!("bucket-request/test-{}.txt", ts),
    );
    file::upload(
        config.s3(),
        &file,
        SdkBody::from(bucket_name.as_bytes()),
        content_type::TEXT_PLAIN,
    )
    .await
    .unwrap();

    let opts = bucket_request::PerformOptions::default();
    bucket_request::perform(&config, &file, &opts)
        .await
        .unwrap();

    assert!(bucket::exists(config.s3(), &primary).await);
    assert!(bucket::exists(config.s3(), &repl).await);

    assert!(
        !file::exists(config.s3(), &file).await,
        "request file should be deleted after processing"
    );

    bucket::empty(config.s3(), &primary).await.unwrap();
    bucket::empty(config.s3(), &repl).await.unwrap();
    bucket::delete(config.s3(), &primary).await.unwrap();
    bucket::delete(config.s3(), &repl).await.unwrap();
}
