// Prefixes and suffixes
pub const BATCH_CHECKSUM_PREFIX: &str = "batch/reports/checksum";
pub const BATCH_MANIFEST_PREFIX: &str = "batch/manifests";
pub const BATCH_POLICY_SUFFIX: &str = "-s3-batch-policy";
pub const BATCH_REPORT_PREFIX: &str = "batch/reports";
pub const BATCH_ROLE_SUFFIX: &str = "-s3-batch-role";
pub const BUCKET_REQUEST_PREFIX: &str = "buckets";
pub const FEEDBACK_PREFIX: &str = "feedback";
pub const LOGGING_PREFIX: &str = "audit";
pub const MANAGED_SUFFIX: &str = "-managed";
pub const MANIFESTS_PREFIX: &str = "manifests";
pub const METADATA_PREFIX: &str = "metadata";
pub const PUBLIC_SUFFIX: &str = "-public";
pub const REPLICATION_POLICY_SUFFIX: &str = "-s3-replication-policy";
pub const REPLICATION_ROLE_SUFFIX: &str = "-s3-replication-role";
pub const REPLICATION_SUFFIX: &str = "-repl";
pub const REPORTS_PREFIX: &str = "reports";
pub const REQUEST_SUFFIX: &str = "-request";
pub const STORAGE_CAPACITY_SUFFIX: &str = "-storage-capacity";
pub const SYNC_USERS_PREFIX: &str = "sync-users";

// Inventory
pub const INVENTORY_ID: &str = "inventory";

// Bucket naming rules
pub const BUCKET_NAME_MIN_PARTS: usize = 3;
pub const DISALLOWED_AFFIXES: &[&str] = &[".", "-"];
pub const STACK_BUCKET_DELIMITER: &str = "-";

// Bucket request rules
pub const MAX_BUCKETS_PER_REQUEST: u8 = 5;
pub const MAX_REQUEST_FILE_SIZE: u16 = 512;
pub const MAX_LEN_FOR_NAME: u8 = 63;

// Bucket tagging
pub const BUCKET_TAG_ORIGIN_KEY: &str = "BucketOrigin";
pub const BUCKET_TAG_ORIGIN_VAL: &str = "bucket-request";
pub const BUCKET_TAG_STACK_KEY: &str = "Stack";
pub const BUCKET_TAG_TRANSITION_STORAGE_CLASS_KEY: &str = "TransitionStorageClass";
pub const BUCKET_TAG_TYPE_KEY: &str = "BucketType";

// Bucket lifecycle
pub const EXPIRE_ABORTED_MULTIPART_DAYS: u8 = 3;
pub const EXPIRE_LEGACY_DURACLOUD_FILE_DAYS: u8 = 3;
pub const EXPIRE_NONCURRENT_VERSION_DAYS: u8 = 14;
pub const LIFECYCLE_LEGACY_DURACLOUD_FILE_TAG_KEY: &str = "LegacyDuraCloudFile";
pub const LIFECYCLE_LEGACY_DURACLOUD_FILE_TAG_VAL: &str = "true";
pub const STORAGE_TRANSITION_DAYS: u8 = 7;

// Bucket replication
pub const REPLICATION_RULE_ID: &str = "ReplicateAll";
pub const REPLICATION_RULE_PRIORITY: i32 = 1;
pub const REPLICATION_TIME_MINUTES: i32 = 15;

// Content types
pub const APPLICATION_JSON: &str = "application/json";
pub const TEXT_CSV: &str = "text/csv";
pub const TEXT_HTML: &str = "text/html";
pub const TEXT_PLAIN: &str = "text/plain";
pub const TEXT_XML: &str = "text/xml";

// Users
pub const SFTPGO_NAMESPACE: &str = "/sftpgo/";
pub const SYNC_USERS_FILE: &str = "TRIGGER";
pub const USER_ACCESS_KEY_NAMESPACE: &str = "/iam/access_key/";
pub const USER_SECRET_KEY_NAMESPACE: &str = "/iam/secret_key/";
