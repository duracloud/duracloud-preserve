# Releases

Deployments consume two kinds of build artifacts:

- **Lambda zips** — one `bootstrap.zip` per function, in the regional
  `dcp-artifacts-{region}`.
- **The dcp Docker image** — `duracloud/dcp`, used by the ECS scheduled
  tasks (e.g. Archive-It).

Both are published with a derived version so test builds can be deployed
without disturbing what production stacks use.

## Version scheme

There is no semantic versioning and nothing to bump. Every publish derives
its version from git:

```text
<version> = <UTC date>-<short commit hash>[-dirty]
            e.g. 20260710-f34103e
```

The `-dirty` suffix is added when the working tree has uncommitted changes,
so a build of unreleased work can never masquerade as its base commit. The
same string is used as the S3 prefix and the Docker tag.

## Channels

| Channel   | Lambda zips                                       | Docker image              | Written by |
|-----------|---------------------------------------------------|---------------------------|------------|
| versioned | `s3://dcp-artifacts-{region}/v/<version>/<fn>/bootstrap.zip` | `duracloud/dcp:<version>` | every `publish` / `push` |
| stable    | `s3://dcp-artifacts-{region}/<fn>/bootstrap.zip`  | `duracloud/dcp:latest`    | `--channel stable` (what `mise run release` uses) |

Stacks deployed without a pinned version follow the stable channel.
Publishing to the stable channel is refused from a dirty tree, so every
stable artifact is traceable to a commit. Versioned copies are never
deleted, which is what makes rollback possible.

## Testing a build

Publish the artifacts (versioned channel only — the stable channel and any
production stack are untouched):

```bash
# Lambda zips; prints the version, e.g. 20260710-f34103e
mise run publish --profile <profile>

# dcp image, if the image is part of what you are testing
mise run push --profile <profile>
```

Deploy a test stack pinned to that version:

```bash
mise run deploy --stack <stack> --profile <profile> --version 20260710-f34103e
```

`--version` sets both the lambda zip keys (`v/<version>/...`) and the dcp
image tag. Terraform fails at plan time if the version was never published
(the `aws_s3_object` data sources look the keys up), so a typo cannot
deploy stale artifacts.

## Promoting to stable

Once the build is verified, commit, then release from the clean tree:

```bash
mise run release --profile <profile>
```

This rebuilds and publishes everything to **both** channels: the stable
keys and `latest` tag are updated, and an immutable versioned copy is kept
alongside.

## Rolling back

Redeploy pinned to any previously released version:

```bash
mise run deploy --stack <stack> --profile <profile> --version <older-version>
```

To roll the stable channel itself back, check out the corresponding commit
and run `mise run release` from it.
