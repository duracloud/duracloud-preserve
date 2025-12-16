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
- `s=stack` resource prefix used for resource partitioning within an aws account

But note in some contexts a letter may have a different meaning, for example
`f=file` (check the docs or output of `make` for details).

To get started run this task to create an S3 replication IAM role and
two required buckets:

```bash
# choose your own value for s=$stack and p=$profile
make setup s=digipres-dev1 p=default
```

Using the above example this will create:

- `digipres-dev1-s3-replication-role` (i.e. `${stack}-s3-replication-role`)
- `digipres-dev1-bucket-request` (i.e. `${stack}-bucket-request`)
- `digipres-dev1-managed` (i.e. `${stack}-managed`)

The role and buckets are expected to exist and created by Terraform for remote
deployments using the [Terraform module](#) included with this repository.

### bucket-request

This function is used to create s3 buckets with [prefab configuration](#).
To do this it downloads a text file from the `${stack}-bucket-request`
bucket that was uploaded by a user. In production this function is triggered
by an event notification but for development we'll invoke the function with
a sample payload.

Again, use your own values for `s=` and `p=`.

```bash
# Run the function locally, waiting for events
make watch f=bucket-request s=digipres-dev1 p=default

# Upload the sample buckets.txt (or create your own) to the request bucket
# Note: the make tasks are preset to work with the buckets.txt entries
make bucket-request f=bucket-request/files/buckets.txt s=digipres-dev1 p=default

# Copy then edit the sample event file so that bucket name uses the s= prefix
mkdir payloads
cp bucket-request/events/sample.json payloads/bucket-request.json # Update bucket name!

# Send an event payload to the locally running function
make invoke f=bucket-request e=payloads/bucket-request.json
```

Using an unmodified `buckets.txt` this will create four new buckets:

- `digipres-dev1-private` (private s3 bucket)
- `digipres-dev1-private-repl` (private s3 bucket replication destination)
- `digipres-dev1-public` (public s3 bucket)
- `digipres-dev1-public-repl` (private s3 bucket replication destination)

Error variations:

- File too large or invalid (rename some other file `buckets.txt` i.e a jpg)
- Wonky names (too long, invalid characters)
- Too many names (max of five names per request, extras are discarded)

#### Cleanup

```bash
# reset: empties the buckets but does not delete them
make reset s=digipres-dev1 p=default

# teardown: empties and deletes the buckets
make teardown s=digipres-dev1 p=default
```

## Creating a function

```bash
cargo lambda new bucket-request
```

```bash
# Build all or specified pkg (using -p)
cargo lambda build [-p $pkg]

# Run local
cargo lambda watch -p $pkg

# Invoke local with a sample payload
cargo lambda invoke -p $pkg --data-example s3-event

# Invoke local using a json file as payload
cargo lambda invoke -p $pkg --data-file $pkg/events/event.json
```

- [Event payloads](https://github.com/aws/aws-lambda-rust-runtime/tree/main/lambda-events/src/fixtures)
