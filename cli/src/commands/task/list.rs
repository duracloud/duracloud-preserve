use aws_sdk_ecs::types::TaskDefinitionFamilyStatus;
use awsutils::config;
use base::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipres-dev1)
    #[arg(short, long, env = "STACK")]
    stack: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let sdk_config = config::load_defaults().await;
    let ecs = aws_sdk_ecs::Client::new(&sdk_config);
    let scheduler = aws_sdk_scheduler::Client::new(&sdk_config);

    let prefix = format!("{}-", stack.as_str());
    let families = ecs
        .list_task_definition_families()
        .family_prefix(&prefix)
        .status(TaskDefinitionFamilyStatus::Active)
        .send()
        .await?;

    println!("{:<32} {:<24} STATE", "TASK", "SCHEDULE");
    for family in families.families() {
        let task = family.strip_prefix(&prefix).unwrap_or(family);
        let (schedule, state) = match schedule_info(&scheduler, family).await? {
            Some(info) => info,
            None => ("-".to_owned(), "disabled".to_owned()),
        };
        println!("{task:<32} {schedule:<24} {state}");
    }

    Ok(())
}

/// Schedule expression and state for a task, or `None` when the task is
/// deployed without a schedule (`enabled = false` in terraform).
async fn schedule_info(
    scheduler: &aws_sdk_scheduler::Client,
    name: &str,
) -> Result<Option<(String, String)>, Box<dyn std::error::Error>> {
    match scheduler
        .get_schedule()
        .group_name(super::SCHEDULE_GROUP)
        .name(name)
        .send()
        .await
    {
        Ok(schedule) => {
            let expression = schedule.schedule_expression().unwrap_or("-").to_owned();
            let state = schedule
                .state()
                .map(|s| s.as_str().to_lowercase())
                .unwrap_or_else(|| "-".to_owned());
            Ok(Some((expression, state)))
        }
        Err(err) => {
            let err = err.into_service_error();
            if err.is_resource_not_found_exception() {
                Ok(None)
            } else {
                Err(err.into())
            }
        }
    }
}
