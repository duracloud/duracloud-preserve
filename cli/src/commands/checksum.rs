use std::{fs::File, io::BufReader, path::PathBuf};

use clap::Args as ClapArgs;
use crc_fast::CrcAlgorithm;

#[derive(ClapArgs)]
pub struct Args {
    /// File to generate checksum for
    #[arg(short, long)]
    file: PathBuf,
}

// TODO: this is only handling Crc64Nvme for checking vs. AWS S3 default for a single file.
// Could be expanded to support dirs and other algos if necessary, but currently it is not.
// Could also be used to implement a direct checksum comparison with an S3 object.

pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let path = shellexpand::tilde(&args.file.to_string_lossy()).into_owned();

    let checksum = tokio::task::spawn_blocking(move || {
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        apputils::generate_checksum(reader, CrcAlgorithm::Crc64Nvme)
    })
    .await
    .expect("failed to spawn blocking task")?;

    println!("{checksum}");
    Ok(())
}
