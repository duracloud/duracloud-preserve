// Shared helpers for the dev task scripts (run via mise tasks — see mise.toml).
import { spawnSync } from "node:child_process";
import process from "node:process";

export function fail(message) {
  console.error(message);
  process.exit(1);
}

// Run a command, inheriting stdio. Exits with the command's status on failure
// unless allowFailure is set.
export function run(cmd, args, { env, cwd, allowFailure = false } = {}) {
  const result = spawnSync(cmd, args, {
    stdio: "inherit",
    cwd,
    env: env ? { ...process.env, ...env } : process.env,
  });
  if (result.error) fail(`Failed to run ${cmd}: ${result.error.message}`);
  if (result.status !== 0 && !allowFailure) process.exit(result.status ?? 1);
  return result;
}

// Run a command and return its trimmed stdout. Exits on failure.
export function capture(cmd, args, { env, cwd } = {}) {
  const result = spawnSync(cmd, args, {
    encoding: "utf8",
    cwd,
    env: env ? { ...process.env, ...env } : process.env,
  });
  if (result.error) fail(`Failed to run ${cmd}: ${result.error.message}`);
  if (result.status !== 0) {
    if (result.stderr) console.error(result.stderr.trim());
    process.exit(result.status ?? 1);
  }
  return result.stdout.trim();
}

// Derived artifact version: UTC date + short commit hash, with a -dirty
// suffix when the working tree has uncommitted changes. Used as both the
// S3 prefix segment (v/<version>/) and the Docker image tag.
export function artifactVersion() {
  const sha = capture("git", ["rev-parse", "--short", "HEAD"]);
  const dirty = capture("git", ["status", "--porcelain"]) ? "-dirty" : "";
  const date = new Date().toISOString().slice(0, 10).replaceAll("-", "");
  return `${date}-${sha}${dirty}`;
}

// Validate a --channel value and enforce that the stable channel is only
// published from a clean tree (so stable artifacts trace to a commit).
export function requireChannel(channel, version) {
  if (channel !== "versioned" && channel !== "stable") {
    fail(`Invalid --channel '${channel}' (expected versioned or stable)`);
  }
  if (channel === "stable" && version.endsWith("-dirty")) {
    fail("Refusing to publish the stable channel from a dirty tree; commit first.");
  }
}

// Env object setting AWS_PROFILE from a possibly-empty profile value.
export function awsEnv(profile) {
  return profile ? { AWS_PROFILE: profile } : {};
}

// Validate that required option values are non-empty.
export function requireOpts(values, usage) {
  for (const [name, value] of Object.entries(values)) {
    if (!value) fail(`Missing required option --${name}\nUsage: ${usage}`);
  }
}
