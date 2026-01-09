use apputils::StackName;
use aws_sdk_iam::Client as IamClient;
use awsutils::bucket::{Bucket, Name, RequestConfig, Type, bucket_exists};
use awsutils::bucket_creator::BucketCreator;
use awsutils::config::{default_config, request_config};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(long)]
    stack: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = StackName::new(&args.stack)?;

    println!("Setting up stack: {}", stack.as_str());

    let sdk_config = default_config().await;
    let iam_client = IamClient::new(&sdk_config);
    let role_arn = create_or_get_replication_role(&iam_client, &stack).await?;
    println!("Replication role: {}", role_arn);

    let config = request_config(stack.clone()).await;
    println!("Account: {}", config.account_id);

    let managed_bucket = Bucket(Name::new(&stack.managed_bucket())?, Type::Managed);
    if !bucket_exists(&config.s3_client, managed_bucket.name()).await {
        let creator = BucketCreator::new(&config, &managed_bucket);
        creator.create().await?;
        println!("Created bucket: {}", managed_bucket.0.as_str());
    }

    let request_bucket = Bucket(Name::new(&stack.request_bucket())?, Type::Request);
    if !bucket_exists(&config.s3_client, request_bucket.name()).await {
        let creator = BucketCreator::new(&config, &request_bucket);
        creator.create().await?;
        println!("Created bucket: {}", request_bucket.0.as_str());
    }

    set_managed_bucket_policy(&config).await?;
    println!("Set inventory policy on: {}", stack.managed_bucket());

    println!("Setup complete");
    Ok(())
}

async fn create_or_get_replication_role(
    client: &IamClient,
    stack: &StackName,
) -> Result<String, Box<dyn std::error::Error>> {
    let role_name = stack.replication_role_name();
    let policy_name = format!("{}-s3-replication-policy", stack.as_str());

    match client.get_role().role_name(&role_name).send().await {
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

    let create_response = client
        .create_role()
        .role_name(&role_name)
        .assume_role_policy_document(assume_role_policy.to_string())
        .tags(
            aws_sdk_iam::types::Tag::builder()
                .key("Name")
                .value(&role_name)
                .build()?,
        )
        .send()
        .await?;

    let role_arn = create_response
        .role()
        .map(|r| r.arn().to_string())
        .ok_or("Failed to get role ARN")?;

    let replication_policy = serde_json::json!({
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
                "Resource": format!("arn:aws:s3:::{}*-repl/*", stack.as_str())
            }
        ]
    });

    client
        .put_role_policy()
        .role_name(&role_name)
        .policy_name(&policy_name)
        .policy_document(replication_policy.to_string())
        .send()
        .await?;

    Ok(role_arn)
}

async fn set_managed_bucket_policy(
    config: &RequestConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let managed_bucket = config.stack.managed_bucket();

    let policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Sid": "AllowS3InventoryDelivery",
            "Effect": "Allow",
            "Principal": {
                "Service": "s3.amazonaws.com"
            },
            "Action": "s3:PutObject",
            "Resource": format!("arn:aws:s3:::{}/*", managed_bucket),
            "Condition": {
                "StringEquals": {
                    "s3:x-amz-acl": "bucket-owner-full-control",
                    "aws:SourceAccount": &config.account_id
                },
                "ArnLike": {
                    "aws:SourceArn": format!("arn:aws:s3:::{}*", config.stack.as_str())
                }
            }
        }]
    });

    config
        .s3_client
        .put_bucket_policy()
        .bucket(&managed_bucket)
        .policy(policy.to_string())
        .send()
        .await?;

    Ok(())
}
