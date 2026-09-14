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

- [mise](https://mise.jdx.dev/) (installs `node` and `rust` from `mise.toml`)
- [aws cli](#)
- [cargo-lambda](#)
- [terraform](#) 1.1 or later

You must have access to an AWS account. **Caution: costs may be incurred.**

## Setup

There are [mise tasks](https://mise.jdx.dev/tasks/) to wrap `cargo` (et al.) commands for convenience:

These options are used frequently:

- `--function` function name i.e. `bucket-request`
- `--profile` aws profile name i.e. `default`
- `--stack` resource prefix used for identification/partitioning within an aws account

Run `mise tasks` to list all tasks and their options.

To get started run this task to create the base infrastructure:

```bash
# choose your own values for --stack and --profile
mise run setup --stack digipres-dev1 --profile default
```

This task uses Terraform so it must be installed for it to work.

Of most significance for testing using the above example will create:

- `digipres-dev1-s3-batch-role` (i.e. `${stack}-s3-batch-role`)
- `digipres-dev1-s3-replication-role` (i.e. `${stack}-s3-replication-role`)
- `digipres-dev1-request` (i.e. `${stack}-request`)
- `digipres-dev1-managed` (i.e. `${stack}-managed`)

The `managed` bucket will also be assigned a policy that permits it to be
a target for S3 inventory from buckets using the same stack name (prefix).

When `cloudfront_enabled = true`, Terraform also creates a CloudFront
distribution and its bucket pair:

- `digipres-dev1-public` (i.e. `${stack}-public`)
- `digipres-dev1-public-repl` (i.e. `${stack}-public-repl`)

The stack module defaults to `cloudfront_enabled = false`, but the
repository's development `main.tf`, used by `mise run setup`, defaults to
`true`. Set `cloudfront_enabled = false` in `terraform.tfvars` to omit the
distribution and its bucket pair from a development stack. The `public`
bucket provides access to files through CloudFront. User-created buckets
with a `-public` suffix provide direct S3 public access independently of
this setting. See the
[deployment guide](../deploy/README.md) for configuration and upgrade details.

## Testing remotely with Lambda

The base infrastructure is sufficient for testing using the provided
cli. However, no AWS Lambda functions will be deployed by the `setup`
task. If you want to test a full stack deployment including the Lambda
functions then there is a `deploy` task for that:

```bash
mise run deploy --stack digipres-dev1 --profile default
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

Enable `cloudfront_enabled = true` and apply the Terraform configuration
before following these steps. The CloudFront outputs have a value only
when CloudFront is enabled.

```bash
terraform output cloudfront_domain_name
```

This will output something like: `d2vy8bpfecxis5.cloudfront.net`.

```bash
mise run upload --bucket digipres-dev1-public --dir example --file files/buckets.txt --profile default
```

Then access the file in the browser, it should work:

- <https://d2vy8bpfecxis5.cloudfront.net/example/files/buckets.txt>

For production the other Terraform outputs can be used for setting up
a custom domain using [ACM](#), see the [deployment documentation](#) for
more details.
