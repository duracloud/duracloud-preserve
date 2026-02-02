use apputils::Stack;
use aws_sdk_iam::Client as IamClient;
use awsutils::bucket::{Bucket, Type, exists};
use awsutils::bucket_creator::BucketCreator;
use awsutils::config::{RequestConfig, default_config, request_config};
use clap::Args as ClapArgs;
use serde_json::Value as JsonValue;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;

    println!("Setting up stack: {}", stack.as_str());

    let sdk_config = default_config().await;
    let iam_client = IamClient::new(&sdk_config);

    let batch_role_arn = create_or_get_batch_role(&iam_client, &stack).await?;
    println!("Batch operations role: {}", batch_role_arn);

    let replication_role_arn = create_or_get_replication_role(&iam_client, &stack).await?;
    println!("Replication role: {}", replication_role_arn);

    let config = request_config(stack.clone()).await;
    println!("Account: {}", config.account_id());

    let managed_bucket = Bucket::new(&stack.managed_bucket(), Type::Managed)?;
    if !exists(&config.client, managed_bucket.name()).await {
        let creator = BucketCreator::new(&config, &managed_bucket);
        creator.create().await?;
        println!("Created bucket: {}", managed_bucket.name());
    }

    let request_bucket = Bucket::new(&stack.request_bucket(), Type::Request)?;
    if !exists(&config.client, request_bucket.name()).await {
        let creator = BucketCreator::new(&config, &request_bucket);
        creator.create().await?;
        println!("Created bucket: {}", request_bucket.name());
    }

    set_managed_bucket_policy(&config).await?;
    println!("Set inventory policy on: {}", stack.managed_bucket());

    println!("Setup complete");
    Ok(())
}

/// Retrieves or creates the stack batch operations role
async fn create_or_get_batch_role(
    client: &IamClient,
    stack: &Stack,
) -> Result<String, Box<dyn std::error::Error>> {
    let assume_role_policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Effect": "Allow",
            "Principal": {
                "Service": "batchoperations.s3.amazonaws.com"
            },
            "Action": "sts:AssumeRole"
        }]
    });

    let permissions_policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [
            {
                "Effect": "Allow",
                "Action": [
                    "s3:GetObject",
                    "s3:GetObjectVersion",
                    "s3:RestoreObject"
                ],
                "Resource": format!("arn:aws:s3:::{}*/*", stack.as_str())
            },
            {
                "Effect": "Allow",
                "Action": "s3:PutObject",
                "Resource": format!("arn:aws:s3:::{}/*", stack.managed_bucket())
            },
            {
                "Effect": "Allow",
                "Action": "s3:PutInventoryConfiguration",
                "Resource": format!("arn:aws:s3:::{}*", stack.as_str())
            }
        ]
    });

    create_or_get_role(
        client,
        &stack.batch_role_name(),
        &stack.batch_policy_name(),
        assume_role_policy,
        permissions_policy,
    )
    .await
}

/// Retrieves or creates the stack replication role
async fn create_or_get_replication_role(
    client: &IamClient,
    stack: &Stack,
) -> Result<String, Box<dyn std::error::Error>> {
    let assume_role_policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Effect": "Allow",
            "Principal": {
                "Service": "s3.amazonaws.com"
            },
            "Action": "sts:AssumeRole"
        }]
    });

    let permissions_policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [
            {
                "Effect": "Allow",
                "Action": [
                    "s3:GetReplicationConfiguration",
                    "s3:ListBucket"
                ],
                "Resource": format!("arn:aws:s3:::{}*", stack.as_str())
            },
            {
                "Effect": "Allow",
                "Action": [
                    "s3:GetObjectVersion",
                    "s3:GetObjectVersionAcl",
                    "s3:GetObjectVersionTagging"
                ],
                "Resource": format!("arn:aws:s3:::{}*/*", stack.as_str())
            },
            {
                "Effect": "Allow",
                "Action": [
                    "s3:GetObjectVersionTagging",
                    "s3:ReplicateObject",
                    "s3:ReplicateDelete",
                    "s3:ReplicateTags"
                ],
                "Resource": format!("arn:aws:s3:::{}*{}/*", stack.as_str(), awsutils::bucket::REPLICATION_SUFFIX)
            }
        ]
    });

    create_or_get_role(
        client,
        &stack.replication_role_name(),
        &stack.replication_policy_name(),
        assume_role_policy,
        permissions_policy,
    )
    .await
}

/// Creates an IAM role with the given policies, or returns the existing role ARN
async fn create_or_get_role(
    client: &IamClient,
    role_name: &str,
    policy_name: &str,
    assume_role_policy: JsonValue,
    permissions_policy: JsonValue,
) -> Result<String, Box<dyn std::error::Error>> {
    match client.get_role().role_name(role_name).send().await {
        Ok(response) => {
            if let Some(role) = response.role() {
                println!("Role {} already exists", role_name);
                return Ok(role.arn().to_string());
            }
        }
        Err(e) => {
            let is_not_found = e
                .as_service_error()
                .map(|se| se.is_no_such_entity_exception())
                .unwrap_or(false);
            if !is_not_found {
                return Err(format!("Failed to check role: {}", e).into());
            }
        }
    }

    println!("Creating role {}...", role_name);

    let create_response = client
        .create_role()
        .role_name(role_name)
        .assume_role_policy_document(assume_role_policy.to_string())
        .tags(
            aws_sdk_iam::types::Tag::builder()
                .key("Name")
                .value(role_name)
                .build()?,
        )
        .send()
        .await?;

    let role_arn = create_response
        .role()
        .map(|r| r.arn().to_string())
        .ok_or("Failed to get role ARN")?;

    client
        .put_role_policy()
        .role_name(role_name)
        .policy_name(policy_name)
        .policy_document(permissions_policy.to_string())
        .send()
        .await?;

    Ok(role_arn)
}

/// Applies the managed bucket policy
async fn set_managed_bucket_policy(
    config: &RequestConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let managed_bucket = config.stack().managed_bucket();

    let policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Sid": "AllowS3DeliveryFromStack",
            "Effect": "Allow",
            "Principal": {
                "Service": ["s3.amazonaws.com", "logging.s3.amazonaws.com"]
            },
            "Action": "s3:PutObject",
            "Resource": format!("arn:aws:s3:::{}/*", managed_bucket),
            "Condition": {
                "StringEquals": {
                    "aws:SourceAccount": &config.account_id()
                },
                "ArnLike": {
                    "aws:SourceArn": format!("arn:aws:s3:::{}*", config.stack().as_str())
                }
            }
        }]
    });

    config
        .client
        .put_bucket_policy()
        .bucket(&managed_bucket)
        .policy(policy.to_string())
        .send()
        .await?;

    Ok(())
}
