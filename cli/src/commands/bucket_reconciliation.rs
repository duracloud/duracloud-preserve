use app::{
    config,
    perform::bucket_reconciliation::{self, PerformOptions},
};
use awsutils::bucket_reconciliator::StepStatus;
use base::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;
    let config = config::load(stack).await?;

    println!("Evaluating buckets for stack: {}", config.stack().as_str());

    let opts = PerformOptions {
        fail_on_drift: false,
    };

    let report = bucket_reconciliation::perform(&config, &opts).await?;

    if report.processed == 0 {
        println!("No bucket-request buckets found for reconciliation");
        return Ok(());
    }

    println!(
        "Found {} bucket-request bucket(s) for reconciliation\n",
        report.processed
    );

    for bucket_report in &report.bucket_reports {
        println!(
            "{} ({})",
            bucket_report.bucket_name, bucket_report.bucket_type
        );

        for step in &bucket_report.steps {
            println!("\t{:18} {}", format!("{}:", step.name), step.status);

            if let StepStatus::Error(msg) = &step.status {
                let compact_msg = msg.split_whitespace().collect::<Vec<_>>().join(" ");
                println!("\t{:18} {}", "", compact_msg);
            }
        }

        let result = if bucket_report.has_errors() {
            "ERROR"
        } else if bucket_report.has_drift() {
            "DRIFT"
        } else {
            "OK"
        };
        println!("\t{:18} {}\n", "result:", result);
    }

    println!(
        "Summary: {} buckets processed, {} ok, {} drift, {} errors",
        report.processed, report.ok, report.drift, report.errors
    );

    if report.has_errors() {
        return Err(format!("reconciliation reported {} error(s)", report.errors).into());
    }

    Ok(())
}
