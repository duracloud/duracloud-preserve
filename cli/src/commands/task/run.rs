use std::time::Duration;

use aws_sdk_ecs::types as ecs;
use awsutils::config;
use base::Stack;
use clap::Args as ClapArgs;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipres-dev1)
    #[arg(short, long, env = "STACK")]
    stack: String,

    /// Task short name (e.g., archive-it-sync)
    task: String,

    /// Stream logs until the task stops and report its exit code
    #[arg(long)]
    follow: bool,

    /// Subnet ID, repeatable (only needed when the task has no schedule to replay)
    #[arg(long = "subnet")]
    subnets: Vec<String>,

    /// Security group ID, repeatable (only needed when the task has no schedule to replay)
    #[arg(long = "security-group")]
    security_groups: Vec<String>,

    /// Container command override, e.g. `-- ait audit --dry-run`
    #[arg(last = true)]
    command: Vec<String>,
}

/// RunTask inputs, normally replayed verbatim from the task's EventBridge
/// Scheduler target so on-demand runs match scheduled ones exactly.
struct LaunchParams {
    task_definition: String,
    subnets: Vec<String>,
    security_groups: Vec<String>,
    assign_public_ip: ecs::AssignPublicIp,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let sdk_config = config::load_defaults().await;
    let ecs_client = aws_sdk_ecs::Client::new(&sdk_config);
    let scheduler = aws_sdk_scheduler::Client::new(&sdk_config);

    let cluster = super::cluster_name(stack.as_str());
    let family = super::family_name(stack.as_str(), &args.task);

    let params = match from_schedule(&scheduler, &family).await? {
        Some(params) => {
            tracing::info!(schedule = %family, "Replaying schedule target");
            params
        }
        None => from_flags(&ecs_client, &family, &args).await?,
    };

    let vpc = ecs::AwsVpcConfiguration::builder()
        .set_subnets(Some(params.subnets))
        .set_security_groups(Some(params.security_groups))
        .assign_public_ip(params.assign_public_ip)
        .build()?;

    let mut request = ecs_client
        .run_task()
        .cluster(&cluster)
        .task_definition(&params.task_definition)
        .launch_type(ecs::LaunchType::Fargate)
        .network_configuration(
            ecs::NetworkConfiguration::builder()
                .awsvpc_configuration(vpc)
                .build(),
        )
        .started_by(started_by());

    if !args.command.is_empty() {
        tracing::info!(command = ?args.command, "Overriding container command");
        request = request.overrides(
            ecs::TaskOverride::builder()
                .container_overrides(
                    ecs::ContainerOverride::builder()
                        .name(&args.task)
                        .set_command(Some(args.command.clone()))
                        .build(),
                )
                .build(),
        );
    }

    let response = request.send().await?;
    if let Some(failure) = response.failures().first() {
        return Err(format!(
            "RunTask failed: {} ({})",
            failure.reason().unwrap_or("unknown reason"),
            failure.detail().unwrap_or("no detail"),
        )
        .into());
    }
    let task_arn = response
        .tasks()
        .first()
        .and_then(|t| t.task_arn())
        .ok_or("RunTask returned no tasks")?
        .to_owned();
    let task_id = task_arn.rsplit('/').next().unwrap_or(&task_arn);

    let region = sdk_config.region().map(|r| r.as_ref()).unwrap_or("");
    tracing::info!(
        task_arn = %task_arn,
        console = %format!(
            "https://{region}.console.aws.amazon.com/ecs/v2/clusters/{cluster}/tasks/{task_id}"
        ),
        "Task launched"
    );

    if !args.follow {
        return Ok(());
    }

    follow(
        &sdk_config,
        &ecs_client,
        &cluster,
        &family,
        &args.task,
        &task_arn,
        task_id,
    )
    .await
}

/// Fetch the task's EventBridge Scheduler schedule and extract its RunTask
/// parameters. `None` when no schedule exists (task deployed with
/// `enabled = false`).
async fn from_schedule(
    scheduler: &aws_sdk_scheduler::Client,
    name: &str,
) -> Result<Option<LaunchParams>, Box<dyn std::error::Error>> {
    let schedule = match scheduler
        .get_schedule()
        .group_name(super::SCHEDULE_GROUP)
        .name(name)
        .send()
        .await
    {
        Ok(schedule) => schedule,
        Err(err) => {
            let err = err.into_service_error();
            if err.is_resource_not_found_exception() {
                return Ok(None);
            }
            return Err(err.into());
        }
    };

    let target = schedule.target().ok_or("schedule has no target")?;
    let params = target
        .ecs_parameters()
        .ok_or("schedule target has no ECS parameters")?;
    let net = params
        .network_configuration()
        .and_then(|n| n.awsvpc_configuration())
        .ok_or("schedule target has no awsvpc network configuration")?;

    let assign_public_ip = match net.assign_public_ip() {
        Some(aws_sdk_scheduler::types::AssignPublicIp::Enabled) => ecs::AssignPublicIp::Enabled,
        _ => ecs::AssignPublicIp::Disabled,
    };

    Ok(Some(LaunchParams {
        task_definition: params.task_definition_arn().to_owned(),
        subnets: net.subnets().to_vec(),
        security_groups: net.security_groups().to_vec(),
        assign_public_ip,
    }))
}

/// Fallback for tasks without a schedule: latest ACTIVE task definition of
/// the family plus explicitly provided network flags.
async fn from_flags(
    ecs_client: &aws_sdk_ecs::Client,
    family: &str,
    args: &Args,
) -> Result<LaunchParams, Box<dyn std::error::Error>> {
    if args.subnets.is_empty() || args.security_groups.is_empty() {
        return Err(format!(
            "no schedule found for {family}; pass --subnet and --security-group \
             to launch it directly (the task is likely deployed with enabled = false)"
        )
        .into());
    }

    let task_definition = ecs_client
        .describe_task_definition()
        .task_definition(family)
        .send()
        .await?
        .task_definition()
        .and_then(|td| td.task_definition_arn())
        .ok_or_else(|| format!("no ACTIVE task definition found for {family}"))?
        .to_owned();

    Ok(LaunchParams {
        task_definition,
        subnets: args.subnets.clone(),
        security_groups: args.security_groups.clone(),
        assign_public_ip: ecs::AssignPublicIp::Enabled,
    })
}

/// Poll the task and stream its CloudWatch logs until it stops; error on a
/// non-zero container exit so the CLI's exit code reflects the task's.
async fn follow(
    sdk_config: &aws_config::SdkConfig,
    ecs_client: &aws_sdk_ecs::Client,
    cluster: &str,
    family: &str,
    task: &str,
    task_arn: &str,
    task_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let logs = aws_sdk_cloudwatchlogs::Client::new(sdk_config);
    // Log group/stream naming from terraform/modules/stack/tasks.tf.
    let log_group = format!("/aws/ecs/{family}");
    let log_stream = format!("{task}/{task}/{task_id}");
    let mut token: Option<String> = None;

    loop {
        token = print_new_events(&logs, &log_group, &log_stream, token).await?;

        let described = ecs_client
            .describe_tasks()
            .cluster(cluster)
            .tasks(task_arn)
            .send()
            .await?;
        let described = described
            .tasks()
            .first()
            .ok_or("task not found while polling")?;

        if described.last_status() == Some("STOPPED") {
            // One last fetch: logs written between the previous fetch and stop.
            print_new_events(&logs, &log_group, &log_stream, token).await?;
            return stopped_result(described, task);
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Print any log events after `token`, returning the new token. Tolerates the
/// stream not existing yet (container hasn't started writing).
async fn print_new_events(
    logs: &aws_sdk_cloudwatchlogs::Client,
    log_group: &str,
    log_stream: &str,
    token: Option<String>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let response = match logs
        .get_log_events()
        .log_group_name(log_group)
        .log_stream_name(log_stream)
        .start_from_head(true)
        .set_next_token(token.clone())
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            let err = err.into_service_error();
            if err.is_resource_not_found_exception() {
                return Ok(token);
            }
            return Err(err.into());
        }
    };

    for event in response.events() {
        if let Some(message) = event.message() {
            println!("{message}");
        }
    }
    Ok(response.next_forward_token().map(str::to_owned))
}

fn stopped_result(
    task: &ecs::Task,
    container_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let container = task
        .containers()
        .iter()
        .find(|c| c.name() == Some(container_name));
    let exit_code = container.and_then(|c| c.exit_code());
    let stopped_reason = task.stopped_reason().unwrap_or("");

    match exit_code {
        Some(0) => {
            tracing::info!("Task completed successfully (exit code 0)");
            Ok(())
        }
        Some(code) => Err(format!("task exited with code {code}: {stopped_reason}").into()),
        None => Err(format!("task stopped without an exit code: {stopped_reason}").into()),
    }
}

/// `startedBy` marker distinguishing on-demand runs from scheduled ones
/// (36-char ECS limit).
fn started_by() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_owned());
    let mut marker = format!("dcp-cli/{user}");
    marker.truncate(36);
    marker
}
