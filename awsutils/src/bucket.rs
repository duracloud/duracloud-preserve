const MANAGED_SUFFIX: &str = "-managed";

pub struct BucketRequestConfig {
    pub debug: bool,
    pub stack: String,
}

impl BucketRequestConfig {
    pub fn managed_bucket(&self) -> String {
        format!("{}-{}", self.stack, MANAGED_SUFFIX)
    }
}
