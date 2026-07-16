// Upload a file to a bucket. The S3 key mirrors the local file path.
// Usage: node scripts/upload.mjs --bucket=<bucket> [--dir=<dir>] --file=<file> [--profile=<aws-profile>]
import { parseArgs } from "node:util";
import { awsEnv, requireOpts, run } from "./lib.mjs";

const USAGE =
  "node scripts/upload.mjs --bucket=<bucket> [--dir=<dir>] --file=<file> [--profile=<aws-profile>]";

const { values } = parseArgs({
  options: {
    bucket: { type: "string", default: "" },
    dir: { type: "string", default: "" },
    file: { type: "string", default: "" },
    profile: { type: "string", default: "" },
  },
});
requireOpts({ bucket: values.bucket, file: values.file }, USAGE);

const key = values.file.replaceAll("\\", "/").replace(/^(\.\/)+/, "");
const prefix = values.dir ? `/${values.dir}` : "";
run("aws", ["s3", "cp", values.file, `s3://${values.bucket}${prefix}/${key}`], {
  env: awsEnv(values.profile),
});
