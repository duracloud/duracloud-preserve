# Setup

This documentation is focused on the technical aspects of the
core functionality and how to test locally using the provided
cli and remotely after the functions have been deployed.

This documentation does not address user functionality or
deployment concerns, for those see:

- [User guide](#)
- [Deployment guide](#)

## Pre-reqs

Requirements:

- [aws cli](#)
- [rust](#)
- [cargo-lambda](#)
- [terraform](#)

You must have access to an AWS account. **Caution: costs may be incurred.**

## Setup

There are `Makefile` tasks to wrap `cargo` (et al.) commands for convenience:

These args are used frequently:

- `f=function` function name i.e. `bucket-request`
- `p=profile` aws profile name i.e. `default`
- `s=stack` resource prefix used for identification/partitioning within an aws account

But note in some contexts a letter may have a different meaning, for example
`f=file` (check the docs or output of `make` for details).

To get started run this task to create the base infrastructure:

```bash
# choose your own value for s=$stack and p=$profile
make setup s=digipres-dev1 p=default
```

This task uses Terraform so it must be installed for it to work.

Of most significance for testing using the above example will create:

- `digipres-dev1-s3-batch-role` (i.e. `${stack}-s3-batch-role`)
- `digipres-dev1-s3-replication-role` (i.e. `${stack}-s3-replication-role`)
- `digipres-dev1-request` (i.e. `${stack}-request`)
- `digipres-dev1-managed` (i.e. `${stack}-managed`)
- `digipres-dev1-public` (i.e. `${stack}-public`)
- `digipres-dev1-public-repl` (i.e. `${stack}-public-repl`)

The `managed` bucket will also be assigned a policy that permits it to be
a target for S3 inventory from buckets using the same stack name (prefix).

The `public` bucket is "special" as it works differently from regular
user created public buckets owing to a CloudFront distribution that is
created to provide access to the files, rather than using raw S3 urls.

## Testing remotely with Lambda

The base infrastructure is sufficient for testing using the provided
cli. However, no AWS Lambda functions will be deployed by the `setup`
task. If you want to test a full stack deployment including the Lambda
functions then there is a `deploy` task for that:

```bash
make deploy s=digipres-dev1 p=default
```

This will build the Lambda packages and upload them to an "artifacts"
bucket that Lambda can access. Doing this will enable you to try
out the remote testing instructions for each function vs. only testing
via the cli. Generally speaking the cli covers most of what happens
when run through Lambda with these primary differences:

- Local cli testing uses your local AWS credentials
- Deployed Lambdas use permissions provided by IAM roles
- The entrypoints are different: see the `cli` vs. `functions` folders

## Testing public access via CloudFront

```bash
terraform output cloudfront_domain_name
```

This will output something like: `d2vy8bpfecxis5.cloudfront.net`.

```bash
make upload b=digipres-dev1-public d=example f=files/buckets.txt p=default
```

Then access the file in the browser, it should work:

- <https://d2vy8bpfecxis5.cloudfront.net/example/files/buckets.txt>

For production the other Terraform outputs can be used for setting up
a custom domain using [ACM](#), see the [deployment documentation](#) for
more details.
