// Manage S3 buckets: list, create, empty, or delete.
// Usage: node scripts/bucket.mjs <list|create|empty|delete> [--bucket=<name>] [--profile=<aws-profile>]
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseArgs } from "node:util";
import { awsEnv, capture, fail, run } from "./lib.mjs";

const USAGE =
  "node scripts/bucket.mjs <list|create|empty|delete> [--bucket=<name>] [--profile=<aws-profile>]";

const { values, positionals } = parseArgs({
  options: {
    bucket: { type: "string", default: "" },
    profile: { type: "string", default: "" },
  },
  allowPositionals: true,
});

const action = positionals[0] ?? "";
const bucket = values.bucket;
const env = awsEnv(values.profile);

if (!["list", "create", "empty", "delete"].includes(action)) {
  fail(`Invalid action '${action}'. Use list, create, empty, or delete.\nUsage: ${USAGE}`);
}
if (action !== "list" && !bucket) {
  fail(`Error: --bucket is required for the ${action} action\nUsage: ${USAGE}`);
}

// Delete one page of versions or delete markers; returns the number deleted.
function deleteVersionPage(field) {
  const out = capture(
    "aws",
    [
      "s3api",
      "list-object-versions",
      "--bucket",
      bucket,
      "--query",
      `{Objects: ${field}[].{Key:Key,VersionId:VersionId} || \`[]\`}`,
      "--output",
      "json",
    ],
    { env },
  );
  const objects = (out ? JSON.parse(out).Objects : null) ?? [];
  if (objects.length === 0) return 0;

  const dir = mkdtempSync(join(tmpdir(), "dcp-bucket-"));
  try {
    const file = join(dir, "delete.json");
    writeFileSync(file, JSON.stringify({ Objects: objects }));
    run(
      "aws",
      [
        "s3api",
        "delete-objects",
        "--bucket",
        bucket,
        "--delete",
        `file://${file.replaceAll("\\", "/")}`,
      ],
      { env },
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  return objects.length;
}

switch (action) {
  case "list": {
    const out = capture(
      "aws",
      ["s3api", "list-buckets", "--query", "Buckets[].Name", "--output", "json"],
      { env },
    );
    for (const name of JSON.parse(out || "[]")) console.log(name);
    break;
  }
  case "create":
    run("aws", ["s3", "mb", `s3://${bucket}`], { env, allowFailure: true });
    break;
  case "empty":
    // list-object-versions returns at most 1000 entries per call, so loop
    while (deleteVersionPage("Versions") > 0);
    while (deleteVersionPage("DeleteMarkers") > 0);
    run("aws", ["s3", "rm", `s3://${bucket}`, "--recursive"], { env });
    break;
  case "delete":
    run("aws", ["s3", "rb", `s3://${bucket}`, "--force"], { env });
    break;
}
