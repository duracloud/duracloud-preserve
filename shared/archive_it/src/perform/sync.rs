use crate::errors::ArchiveItError;

#[derive(Debug, Clone)]
pub struct PerformArgs {}

pub async fn perform(_args: &PerformArgs) -> Result<(), ArchiveItError> {
    todo!()
}
