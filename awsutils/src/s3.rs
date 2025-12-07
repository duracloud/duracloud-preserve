pub fn download(_: File) {
    todo!()
}

pub fn stream(_: File) {
    todo!()
}

pub fn upload(_: File) {
    todo!()
}

pub struct File {
    bucket: String,
    object: String,
}

impl File {
    pub fn new(bucket: String, object: String) -> Self {
        Self { bucket, object }
    }

    pub fn http_url(&self) -> String {
        todo!()
    }

    pub fn s3_url(&self) -> String {
        format!("s3://{}/{}", self.bucket, self.object)
    }
}
