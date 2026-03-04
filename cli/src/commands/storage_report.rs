use apputils::Stack;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    /// Stack name (e.g., digipress-dev1)
    #[arg(short, long)]
    stack: String,
}

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let stack = Stack::new(&args.stack)?;

    println!("{:?}", stack);
    Ok(())
}
