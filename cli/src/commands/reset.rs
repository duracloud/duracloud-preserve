use std::io::{self, Write};

use apputils::Stack;
use aws_sdk_iam::Client as IamClient;
use awsutils::bucket;
use awsutils::config::default_config;
use clap::Args as ClapArgs;
use rand::Rng;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,

    /// Delete resources after emptying (default: false)
    #[arg(short, long, default_value = "false")]
    destroy: bool,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let sdk_config = default_config().await;
    let s3_client = aws_sdk_s3::Client::new(&sdk_config);
    let iam_client = IamClient::new(&sdk_config);

    println!("Discovering buckets for stack: {}", stack.as_str());

    let buckets = bucket::get_stack_buckets(&s3_client, &stack).await?;

    if buckets.is_empty() {
        println!("No buckets found for stack {}", stack.as_str());
        if !args.destroy {
            return Ok(());
        }
    } else {
        println!("\nFound {} bucket(s):", buckets.len());
        for bucket in &buckets {
            println!("\t{} ({})", bucket.name(), bucket.bucket_type());
        }
    }

    if args.destroy {
        println!("\nPlanned actions:");
        if !buckets.is_empty() {
            println!("\t- Empty all bucket contents");
            println!("\t- Delete all buckets");
        }
        println!("\t- Delete IAM roles (replication, batch)");
    } else {
        println!("\nPlanned actions:");
        println!("\t- Empty all bucket contents");
    }

    let code = generate_confirmation_code();
    println!("\nTo proceed, enter this code: {}", code);
    print!("Confirmation: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim() != code {
        println!("Code does not match. Aborting.");
        return Ok(());
    }

    for b in &buckets {
        let name = b.name();

        println!("\nProcessing bucket: {}", name);

        print!("\tEmptying bucket... ");
        io::stdout().flush()?;
        bucket::empty(&s3_client, name).await?;
        println!("done");
    }

    if args.destroy {
        for b in &buckets {
            let name = b.name();
            print!("\tDeleting bucket {}... ", name);
            io::stdout().flush()?;
            bucket::delete(&s3_client, name).await?;
            println!("done");
        }

        print!("Deleting batch role... ");
        io::stdout().flush()?;
        delete_role(
            &iam_client,
            &stack.batch_role_name(),
            &stack.batch_policy_name(),
        )
        .await?;
        println!("done");

        print!("\nDeleting replication role... ");
        io::stdout().flush()?;
        delete_role(
            &iam_client,
            &stack.replication_role_name(),
            &stack.replication_policy_name(),
        )
        .await?;
        println!("done");

        println!("\nAll stack resources destroyed");
    } else {
        println!("\nAll buckets emptied");
    }

    Ok(())
}

/// Delete an IAM role and its associated policy
async fn delete_role(
    client: &IamClient,
    role_name: &str,
    policy_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match client
        .delete_role_policy()
        .role_name(role_name)
        .policy_name(policy_name)
        .send()
        .await
    {
        Ok(_) => {}
        Err(e) => {
            let is_not_found = e
                .as_service_error()
                .map(|se| se.is_no_such_entity_exception())
                .unwrap_or(false);
            if !is_not_found {
                return Err(format!("Failed to delete role policy: {}", e).into());
            }
        }
    }

    match client.delete_role().role_name(role_name).send().await {
        Ok(_) => {}
        Err(e) => {
            let is_not_found = e
                .as_service_error()
                .map(|se| se.is_no_such_entity_exception())
                .unwrap_or(false);
            if !is_not_found {
                return Err(format!("Failed to delete role: {}", e).into());
            }
        }
    }

    Ok(())
}

/// Generate confirmation code for user input
fn generate_confirmation_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();

    (0..6)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
