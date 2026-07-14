// Publish lambda release artifacts to the dcp-artifacts buckets.
// Every run publishes under a versioned prefix (v/<version>/); the stable
// channel additionally updates the unversioned keys production stacks
// deploy from. See docs/src/technical/releases.md.
// Usage: node scripts/publish.mjs [--profile=<aws-profile>] [--channel=<versioned|stable>]
import { parseArgs } from "node:util";
import { artifactVersion, awsEnv, requireChannel, run } from "./lib.mjs";

const ARTIFACT_REGIONS = ["us-east-1", "us-east-2", "us-west-2"];

const { values } = parseArgs({
  options: {
    profile: { type: "string", default: "" },
    channel: { type: "string", default: "versioned" },
  },
});
const env = awsEnv(values.profile);
const version = artifactVersion();
requireChannel(values.channel, version);

const destinations = [`v/${version}/`];
if (values.channel === "stable") destinations.push("");

for (const region of ARTIFACT_REGIONS) {
  for (const dest of destinations) {
    console.log(`Publishing to dcp-artifacts-${region}/${dest}...`);
    run(
      "aws",
      [
        "s3",
        "sync",
        "target/lambda/",
        `s3://dcp-artifacts-${region}/${dest}`,
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
}

console.log(`\nPublished version: ${version}`);
