// Build and push the dcp Docker image with derived version tags.
// build tags <version> and latest locally (latest is kept for local
// `docker run` convenience); push sends <version>, and latest only on the
// stable channel. See docs/src/technical/releases.md.
// Usage: node scripts/docker.mjs <build|push> [--channel=<versioned|stable>]
import { parseArgs } from "node:util";
import { artifactVersion, fail, requireChannel, run } from "./lib.mjs";

const IMAGE = "duracloud/dcp";
const USAGE = "Usage: node scripts/docker.mjs <build|push> [--channel=<versioned|stable>]";

const { positionals, values } = parseArgs({
  allowPositionals: true,
  options: { channel: { type: "string", default: "versioned" } },
});
const [action] = positionals;
const version = artifactVersion();
requireChannel(values.channel, version);

if (action === "build") {
  run("docker", [
    "buildx",
    "build",
    "--platform",
    "linux/arm64",
    "-t",
    `${IMAGE}:${version}`,
    "-t",
    `${IMAGE}:latest`,
    "--load",
    ".",
  ]);
  console.log(`\nBuilt version: ${version}`);
} else if (action === "push") {
  run("docker", ["push", `${IMAGE}:${version}`]);
  if (values.channel === "stable") run("docker", ["push", `${IMAGE}:latest`]);
  console.log(`\nPushed version: ${version}`);
} else {
  fail(USAGE);
}
