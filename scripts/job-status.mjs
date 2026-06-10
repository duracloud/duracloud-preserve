// Look up an S3 Batch Operations job status, by id or by checksum receipt.
// Usage: node scripts/job-status.mjs --id=<job-id> [--profile=<aws-profile>]
//    or: node scripts/job-status.mjs --bucket=<bucket> [--profile=<aws-profile>]
import { parseArgs } from "node:util";
import { awsEnv, capture, fail, run } from "./lib.mjs";

const USAGE =
  "node scripts/job-status.mjs (--id=<job-id> | --bucket=<bucket>) [--profile=<aws-profile>]";

const { values } = parseArgs({
  options: {
    id: { type: "string", default: "" },
    bucket: { type: "string", default: "" },
    profile: { type: "string", default: "" },
  },
});
const env = awsEnv(values.profile);

let jobId = values.id;
if (!jobId && values.bucket) {
  const bucket = values.bucket;
  const dash = bucket.lastIndexOf("-");
  const stack = dash === -1 ? bucket : bucket.slice(0, dash);
  const receipt = capture(
    "aws",
    [
      "s3",
      "cp",
      `s3://${stack}-managed/metadata/0000-00-00-LATEST/checksums/receipts/${bucket}.json`,
      "-",
    ],
    { env },
  );
  jobId = JSON.parse(receipt).repl_job_id;
  if (!jobId) fail(`No repl_job_id found in checksum receipt for ${bucket}`);
}
if (!jobId) fail(`Missing required option --id or --bucket\nUsage: ${USAGE}`);

const account = capture(
  "aws",
  ["sts", "get-caller-identity", "--query", "Account", "--output", "text"],
  { env },
);
run("aws", ["s3control", "describe-job", "--account-id", account, "--job-id", jobId], { env });
