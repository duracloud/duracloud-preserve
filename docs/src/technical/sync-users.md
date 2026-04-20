# sync-users

- **Lambda trigger:** S3 event (fires when a `TRIGGER` file is uploaded to the managed bucket under `sync-users/`)
- **Dependencies:** None

## Overview

This Lambda function synchronizes IAM users with an [SFTPGo](#) server so that each user can access their stack buckets over SFTP using their AWS access keys.

Unlike the other functions, `sync-users` operates across stacks. A user can belong to one or more stacks (via IAM group membership), and this function discovers those relationships to grant the user access to the appropriate set of buckets.

> [!IMPORTANT]
> `sync-users` only updates existing SFTPGo users — it does not create them. SFTPGo users are provisioned separately via the `users` terraform module.

The workflow is:

1. An empty `TRIGGER` file is uploaded to `s3://${stack}-managed/sync-users/TRIGGER`
2. The Lambda function is triggered by the upload event
3. Eligible IAM users are discovered (those with an `Email` tag and one or more stack group memberships)
4. For each user, their access/secret keys are retrieved from SSM and the matching SFTPGo account is updated with access to the buckets for each stack they belong to
5. The `TRIGGER` file is deleted on success

The SFTPGo connection details (`SFTPGO_HOST`, `SFTPGO_USERNAME`, `SFTPGO_PASSWORD`) are provided via Lambda environment variables set at deploy time.

## CLI testing

The CLI can sync a single user or all users. SFTPGo credentials are read from the environment.

```bash
SFTPGO_HOST=https://sftpgo.example.org \
SFTPGO_USERNAME=admin \
SFTPGO_PASSWORD=secret \
cargo run -p duracloud -- sync-users
```

To sync a specific user only:

```bash
SFTPGO_HOST=... SFTPGO_USERNAME=... SFTPGO_PASSWORD=... \
cargo run -p duracloud -- sync-users --username=alice
```

Unlike other CLI commands, `sync-users` does not take a stack argument — it works across all eligible users in the account.

## Remote testing

Upload the `TRIGGER` file to the managed bucket to invoke the Lambda:

```bash
make upload b=digipres-dev1-managed d=sync-users f=TRIGGER p=default
```

- `b=` — the managed bucket name (`${stack}-managed`)
- `d=` — the S3 directory (must be `sync-users`)
- `f=` — path to an empty local file named `TRIGGER`
- `p=` — the AWS profile to use

Create an empty `TRIGGER` file first if you don't have one:

```bash
touch TRIGGER
```

## Output

`sync-users` does not produce files in S3. Successful execution can be verified in the following ways:

- The `TRIGGER` file is removed from `s3://${stack}-managed/sync-users/` after a successful run
- CloudWatch logs show per-user processing output (email, identified buckets)
- The SFTPGo admin UI shows the expected users with the expected bucket virtual folders configured

## QA testing

Confirm:

- A user with no `Email` tag is skipped (not synced)
- A user with no stack group memberships is skipped (no buckets)
- A user with no matching SFTPGo account is skipped (sync-users does not create SFTPGo users)
- A user belonging to multiple stacks has access to buckets from each stack
- The `TRIGGER` file is deleted after a successful run
- A user's SFTPGo account reflects changes when their IAM group memberships change
