use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    path::PathBuf,
};

use base64::{Engine, engine::general_purpose};
use clap::Args as ClapArgs;
use crc_fast::{CrcAlgorithm, Digest};

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
    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let checksum = generate_checksum(reader, CrcAlgorithm::Crc64Nvme)?;

    println!("{checksum}");
    Ok(())
}

// TODO: if expanding shift this to apputils
fn generate_checksum(mut reader: impl BufRead, algorithm: CrcAlgorithm) -> io::Result<String> {
    let mut buffer = [0u8; 8192];
    let mut digest = Digest::new(algorithm);

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }

    let bytes = digest.finalize().to_be_bytes();

    // Base64 encode to match AWS S3 console
    Ok(general_purpose::STANDARD.encode(bytes))
}
