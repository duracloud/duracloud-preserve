# Development

Most new features follow the same progression: **CLI command → perform module → Lambda → Terraform**. The CLI is the fastest path to a working end-to-end against real AWS, and the Lambda is a thin entrypoint that delegates to the same perform module once the functionality is proven.

## 1. Add a CLI command

The CLI lives in `cli/src/commands/`. Each command is its own module exposing an `Args` struct and a `run` function.

- Create `cli/src/commands/<new_command>.rs` with `pub struct Args` (clap) and `pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>>`.
- Register the module in `cli/src/commands/mod.rs`.
- Add a `Commands::<NewCommand>(commands::<new_command>::Args)` variant and dispatch arm in `cli/src/main.rs`.
- Build SDK clients directly from `awsutils::config::load_defaults()` + `Clients::new(&sdk_config)`, or use `app::config::load(stack)` if the command is stack-scoped.
- Wire clap args/env vars (e.g. `#[arg(long, env = "SFTPGO_HOST")]`).

Keep the CLI thin — parse args, build config, delegate to a perform function.

## 2. Implement the perform module

Shared functionality lives in `shared/app/src/perform/`. This is where the real work happens, and it is reused by both the CLI and the Lambda.

- Create `shared/app/src/perform/<feature>.rs`.
- Export a `PerformArgs` struct (public fields) and `pub async fn perform(...) -> Result<..., <Feature>Error>`.
- Add the module to `shared/app/src/perform/mod.rs`.
- Add a `<Feature>Error` variant in `shared/app/src/errors.rs`.
- If the work is stack-scoped, accept `&Config`. For account-wide work (e.g. cross-stack user sync), accept `&Clients` instead.

Write unit tests alongside the module with `test_support::TestClientBuilder` for mocked SDK responses. Integration tests that hit real AWS go in `shared/app/tests/<feature>.rs` (gated with `#[ignore]` and run via `make test-integration`).

## 3. Add a Lambda function

Once the CLI and perform module work, wrap them in a Lambda entrypoint.

```bash
cd functions
cargo lambda new <feature-name>
```

Add the new crate to `members` in the workspace `Cargo.toml`.

Each Lambda crate has two files:

- `src/main.rs` — reads env vars (at minimum `STACK`), loads config, starts the runtime.
- `src/event_handler.rs` — validates the inbound event (bucket, prefix, filename), short-circuits on `config.debug_handler()`, builds `PerformArgs`, calls `perform`.

Provide a sample payload at `events/sample.json` and test the handler with `test_support::TestClientBuilder` + `debug_handler=true`.

From the project root:

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

## 4. Wire up Terraform

The Lambda needs infrastructure: an IAM policy scoping its permissions, a trigger (S3 event or EventBridge schedule), and an entry in the dev `main.tf` so the artifact gets uploaded and the function gets deployed.

### 4a. Shared constants → terraform locals

If the Lambda needs any prefixes, filenames, or other fixed values that terraform also needs to reference, add them to `shared/constants/src/lib.rs` and regenerate the terraform locals:

```bash
make locals
```

This keeps Rust and Terraform aligned — never hand-edit `terraform/modules/stack/_locals.tf`.

### 4b. Function-specific IAM policy

Create `terraform/modules/stack/<feature>.tf` following the pattern in `bucket_request.tf` or `storage_report.tf`:

```hcl
locals {
  deploy_<feature> = contains(keys(local.functions), "<feature>") ? { "<feature>" = {} } : {}
}

data "aws_iam_policy_document" "<feature>" {
  for_each = local.deploy_<feature>

  statement { ... }
}

resource "aws_iam_role_policy" "<feature>" {
  for_each = local.deploy_<feature>

  role   = aws_iam_role.lambda[each.key].name
  policy = data.aws_iam_policy_document.<feature>[each.key].json
}
```

The base Lambda role, log group, and error alarm are created automatically from the `functions` map in `functions.tf` and `alarms.tf` — you do not need to add those.

### 4c. Trigger

Pick one based on how the function should fire:

**S3 event trigger** — add a `aws_lambda_permission` resource scoped to the source bucket ARN in your `<feature>.tf`, then add an entry to the appropriate bucket in `notifications.tf`:

```hcl
for k, _ in local.deploy_<feature> : {
  id            = "<feature>-trigger"
  lambda_arn    = aws_lambda_function.main[k].arn
  events        = ["s3:ObjectCreated:*"]
  filter_prefix = "${local.<feature>_prefix}/"
  filter_suffix = local.<feature>_file
}
```

Add `aws_lambda_permission.<feature>` to the `depends_on` list.

**Scheduled trigger** — add `local.deploy_<feature>` into `local.scheduled_functions` in `scheduler.tf`. The schedule itself is configured via the `schedule` and `tz` fields on the `functions` map entry (defaults in `variables.tf`).

### 4d. Register in the dev `main.tf`

Add the function to `local.functions` in the project-root `main.tf` so it gets built, uploaded to the artifacts bucket, and deployed:

```hcl
<feature> = {
  bucket = local.functions_bucket
  file   = "target/lambda/<feature>/bootstrap.zip"
  env    = { SOME_VAR = local.some_value } # optional
}
```

### 4e. Apply

```bash
make deploy s=<stack> p=<profile>
```

## Testing the new function

- **CLI (local, against real AWS):** `cargo run -p duracloud -- <subcommand> [args]`
- **Lambda (invoked remotely with sample payload):** `make trigger f=<feature> s=<stack> p=<profile>`
- **Unit tests:** `cargo test -p <crate>`
- **Integration tests:** `make test-integration s=<stack> p=<profile>`

Each feature should also get a technical doc at `docs/src/technical/<feature>.md` following the format of the others in that directory.
