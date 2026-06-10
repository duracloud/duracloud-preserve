// Publish lambda release artifacts to the dcp-artifacts buckets.
// Usage: node scripts/publish.mjs [--profile=<aws-profile>]
import { parseArgs } from "node:util";
import { awsEnv, run } from "./lib.mjs";

const ARTIFACT_REGIONS = ["us-east-1", "us-east-2", "us-west-2"];

const { values } = parseArgs({
  options: { profile: { type: "string", default: "" } },
});
const env = awsEnv(values.profile);

for (const region of ARTIFACT_REGIONS) {
  console.log(`Publishing to dcp-artifacts-${region}...`);
  run(
    "aws",
    [
      "s3",
      "sync",
      "target/lambda/",
      `s3://dcp-artifacts-${region}/`,
      "--region",
      region,
      "--exclude",
      "*",
      "--include",
      "*.zip",
    ],
    { env },
  );
}
