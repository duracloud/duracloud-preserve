# Developer docs

Requirements:

- [rust](#)
- [cargo-lambda](#)

## Testing functions

There are `Makefile` tasks to wrap `cargo` (et al.) commands for convenience:

Common args:

- `f=function` function name i.e. bucket-request
- `p=profile` aws profile name
- `s=stack` resource prefix used for partitioning within aws account

But note in some contexts a letter may have a different meaning (for example
`f=file`, check the docs or output of `make` for details).

To get started run this task to create two required buckets:

```bash
# choose your own value for s=$stack and p=$profile
make setup s=digipres-dev1 p=default
```

In the example this will create:

- `digipres-dev1-bucket-request` (i.e. `${stack}-bucket-request`)
- `digipres-dev1-managed` (i.e. `${stack}-managed`)

These buckets are expected to exist and created by Terraform for remote deployments.

### bucket-request

Again, use your own values for `s=` and `p=`.

```bash
# Run function locally, waiting for events
make watch f=bucket-request s=digipres-dev1 p=default

# Upload the sample buckets.txt (or create your own) to the request bucket
make bucket-request f=bucket-request/files/buckets.txt s=digipres-dev1 p=default

# Copy then edit the sample event file so that bucket name uses the s= prefix
mkdir payloads
cp bucket-request/events/sample.json payloads/bucket-request.json # Update bucket name!

# Send payload to the locally running function
make invoke f=bucket-request e=payloads/bucket-request.json
```

Try variations in `buckets.txt` (file too large, wonky names, too many names etc.).

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
