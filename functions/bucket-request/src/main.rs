use awsutils::bucket_creator;
use lambda_runtime::{run, service_fn, tracing, Error};

mod event_handler;
use event_handler::function_handler;

use apputils::Stack;
use aws_sdk_s3::types::TransitionStorageClass;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let stack =
        Stack::new(&env::var("STACK").expect("Stack is required")).expect("Invalid stack name");

    let config = awsutils::config::config(stack).await;

    let standard_storage_tier = match env::var("STORAGE_TIER") {
        Ok(value) => parse_storage_tier(&value)?,
        Err(env::VarError::NotPresent) => bucket_creator::STORAGE_CLASS_STANDARD_DEFAULT,
        Err(e) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Failed to read STORAGE_TIER: {e}"),
            )
            .into());
        }
    };

    let perform_opts = awsutils::bucket_request::PerformOptions {
        standard_storage_tier,
    };

    run(service_fn(|event| {
        function_handler(&config, &perform_opts, event)
    }))
    .await
}

fn parse_storage_tier(value: &str) -> Result<TransitionStorageClass, std::io::Error> {
    let normalized = value.trim().to_ascii_uppercase();
    let tier = match normalized.as_str() {
        "DEEP_ARCHIVE" => TransitionStorageClass::DeepArchive,
        "GLACIER" => TransitionStorageClass::Glacier,
        "GLACIER_IR" => TransitionStorageClass::GlacierIr,
        "INTELLIGENT_TIERING" => TransitionStorageClass::IntelligentTiering,
        "ONEZONE_IA" => TransitionStorageClass::OnezoneIa,
        "STANDARD_IA" => TransitionStorageClass::StandardIa,
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Invalid STORAGE_TIER '{value}'. Allowed: DEEP_ARCHIVE, GLACIER, GLACIER_IR, INTELLIGENT_TIERING, ONEZONE_IA, STANDARD_IA"
                ),
            ));
        }
    };

    Ok(tier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_storage_tier_valid() {
        let tier = parse_storage_tier("GLACIER_IR").expect("GLACIER_IR should parse");
        assert_eq!(tier, TransitionStorageClass::GlacierIr);
    }

    #[test]
    fn test_parse_storage_tier_invalid() {
        let err = parse_storage_tier("NOT_A_TIER").expect_err("invalid tier should fail");
        assert!(
            err.to_string().contains("Allowed:"),
            "expected allowed values in error: {}",
            err
        );
    }
}
