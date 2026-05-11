use archive_it_client::{PartnerClient, WasapiClient};
use awsutils::{
    bucket,
    file::{self, File},
};
use base::Stack;

use crate::errors::ArchiveItError;

#[derive(Debug, Clone)]
pub struct PerformArgs {
    pub username: String,
    pub password: String,
}

pub async fn perform(
    s3: &aws_sdk_s3::Client,
    stack: &Stack,
    args: &PerformArgs,
) -> Result<(), ArchiveItError> {
    let _partner = PartnerClient::new(args.username.clone(), args.password.clone())?;
    let _wasapi = WasapiClient::new(args.username.clone(), args.password.clone())?;

    let inventory: File = stack.archive_it_inventory().into();

    if !bucket::exists(s3, &stack.archive_it_bucket()).await {
        return Err(ArchiveItError::NotFound("Bucket not found".into()));
    }

    if !file::exists(s3, &inventory).await {
        return Err(ArchiveItError::NotFound("Inventory not found".into()));
    }

    todo!()
}
