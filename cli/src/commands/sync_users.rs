use app::perform::sync_users;
use awsutils::config;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Specific username to sync (syncs all users when omitted)
    #[arg(short, long)]
    username: Option<String>,

    /// SFTPGo server host URL
    #[arg(long, env = "SFTPGO_HOST")]
    sftpgo_host: String,

    /// SFTPGo admin username
    #[arg(long, env = "SFTPGO_USERNAME")]
    sftpgo_username: String,

    /// SFTPGo admin password
    #[arg(long, env = "SFTPGO_PASSWORD")]
    sftpgo_password: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let config = config::load_defaults().await;

    let perform_args = sync_users::PerformArgs {
        username: args.username,
        sftpgo_host: args.sftpgo_host,
        sftpgo_username: args.sftpgo_username,
        sftpgo_password: args.sftpgo_password,
    };

    sync_users::perform(&config, &perform_args).await?;
    Ok(())
}
