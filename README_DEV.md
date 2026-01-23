# Developer docs

Requirements:

- [rust](#)
- [cargo-lambda](#)

This documentation focuses solely on testing and running functions
locally and the requirements needed to support that work. It does
not cover "application" or deployment level concerns such as IAM
user management and access to buckets etc. For the latter refer to
the [deployment](#) and [Terraform module](#) documentation.

## Testing functions

There are `Makefile` tasks to wrap `cargo` (et al.) commands for convenience:

These args are used frequently:

- `f=function` function name i.e. bucket-request
- `p=profile` aws profile name i.e. default
- `s=stack` resource prefix used for identification/partitioning within an aws account

But note in some contexts a letter may have a different meaning, for example
`f=file` (check the docs or output of `make` for details).

To get started run this task to create an S3 batch & replication IAM roles and
two required buckets:

```bash
# choose your own value for s=$stack and p=$profile
make setup s=digipres-dev1 p=default
```

Using the above example this will create:

- `digipres-dev1-s3-batch-role` (i.e. `${stack}-s3-batch-role`)
- `digipres-dev1-s3-replication-role` (i.e. `${stack}-s3-replication-role`)
- `digipres-dev1-bucket-request` (i.e. `${stack}-bucket-request`)
- `digipres-dev1-managed` (i.e. `${stack}-managed`)

The role and buckets are expected to exist and created by Terraform for remote
deployments using the [Terraform module](#) included with this repository.

The `managed` bucket will also be assigned a policy that permits it to be
a target for S3 inventory from buckets using the same stack name (prefix).

### bucket-request

This function is used to create s3 buckets with [prefab configuration](#).
To do this it downloads a text file from the `${stack}-bucket-request`
bucket that was uploaded by a user. In production this function is triggered
by an event notification but for development we can use the cli or invoke the
function locally with a sample payload.

#### CLI

The core functionality of the bucket request function can be exercised
without needing any additional setup within AWS (or the locally running
Lambda function) using the `bucket-request` task:

```bash
make bucket-request f=files/buckets.txt s=digipress-dev1 p=default
```

This can be quicker and simpler for testing with different files.

#### Locally running function

This is close to exactly what runs when deployed to AWS, but we provide the
payload:

```bash
# Run the function locally, waiting for events
make watch f=bucket-request s=digipres-dev1 p=default

# Send an event payload to the locally running function
make invoke-bucket-request s=digipres-dev1 p=default
```

#### Output

Using an unmodified `buckets.txt` will create four new buckets:

- `digipres-dev1-private` (private s3 bucket)
- `digipres-dev1-private-repl` (private s3 bucket replication destination)
- `digipres-dev1-public` (public s3 bucket)
- `digipres-dev1-public-repl` (public s3 bucket replication destination)

Error variations to try out:

- File too large or invalid (rename some other file `buckets.txt` i.e a jpg)
- Wonky names (too long, invalid characters)
- Too many names (max of five names per request, extras are discarded)

### process-inventory

This function processes inventory `manifest.json` files. It generates
and uploads a consolidated (single) CSV file with the https url included
as a column. It also uses the inventory data to generates stats:

- Total number of files and total storage used
- The same, but grouped by top level prefix (folder)

The former is provided by CloudWatch metrics, but the latter is not. The
inventory is used to provide a single path for gathering usage data.

The stats are uploaded as a json file to the managed bucket.

#### CLI

The cli task can be used to process inventory so long as there is
inventory available:

```bash
make process-inventory b=digipress-dev1-private s=digipress-dev1 p=default
```

#### Locally running function

This is close to exactly what runs when deployed to AWS, but we provide the
payload:

```bash
# Run the function locally, waiting for events
make watch f=process-inventory s=digipres-dev1 p=default

# Send an event payload to the locally running function
make invoke-process-inventory s=digipres-dev1 p=default
```

Note: the sample parquet data refers to a `test-stack` bucket, which will appear
in the uploaded CSV file. This can be ignored as real inventory won't contain this
contradiction (between the source bucket and bucket name in the inventory data).
This doesn't apply to cli usage as it processes real inventory manifests and
parquet files.

#### Output

- metadata/latest/stats/$bucket.csv
- metadata/YYYY-MM-DD/stats/$bucket.csv
- reports/latest/manifests/$bucket.csv
- reports/YYYY-MM-DD/manifests/$bucket.csv

## Cleanup

```bash
# reset: empties the buckets but does not delete them
make reset s=digipres-dev1 p=default

# teardown: empties and deletes the buckets and IAM replication role
make teardown s=digipres-dev1 p=default
```

## Creating a function

```bash
cd functions
cargo lambda new bucket-request
```

Add the function path to the project root `Cargo.toml`.

From the project root dir:

```bash
# Build all or specified pkg (using -p)
cargo lambda build [-p $pkg]

# Run local
cargo lambda watch -p $pkg

# Invoke local with a sample payload
cargo lambda invoke -p $pkg --data-example s3-event

# Invoke local using a json file as payload
cargo lambda invoke -p $pkg --data-file functions/$pkg/events/event.json
```

- [Event payloads](https://github.com/aws/aws-lambda-rust-runtime/tree/main/lambda-events/src/fixtures)
