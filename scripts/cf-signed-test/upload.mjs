// Upload .work/assets to the stack's public bucket in the final key layout.
// open-hls/ goes up first. Under the signed base path, a canary object goes up
// next and must be denied unsigned and served signed before anything else
// under the base path is uploaded.
// Usage: node upload.mjs --profile <aws-profile> [--stack dcp-test] [--key test_1] [--outputs path]
import { join } from "node:path";
import { parseArgs } from "node:util";
import {
  ASSETS,
  awsEnv,
  CONFIG_OPTIONS,
  contentType,
  fail,
  fetchRaw,
  loadConfig,
  run,
  signer,
  signParams,
} from "./lib.mjs";

const { values } = parseArgs({
  options: { ...CONFIG_OPTIONS, profile: { type: "string", default: "" } },
});
const config = loadConfig(values);
const env = awsEnv(values.profile);
const basePrefix = config.basePath.slice(1);

// One recursive copy per extension so each gets its content type
function copyTree(localDir, s3Prefix) {
  for (const ext of ["m3u8", "ts"]) {
    run(
      "aws",
      [
        "s3",
        "cp",
        localDir,
        `s3://${config.bucket}/${s3Prefix}`,
        "--recursive",
        "--exclude",
        "*",
        "--include",
        `*.${ext}`,
        "--content-type",
        contentType(`x.${ext}`),
        "--only-show-errors",
      ],
      { env },
    );
  }
}

console.log(`Uploading open-hls/ to ${config.bucket}`);
copyTree(join(ASSETS, "open-hls"), "open-hls/");

const canaryKey = `${basePrefix}canary.txt`;
console.log(`Uploading canary ${canaryKey}`);
run(
  "aws",
  [
    "s3",
    "cp",
    join(ASSETS, "restricted", "canary.txt"),
    `s3://${config.bucket}/${canaryKey}`,
    "--content-type",
    "text/plain",
    "--only-show-errors",
  ],
  { env },
);

// Signed first, so this script's own denied request can't affect it. A 404
// cached for 60s from before the canary existed is served to signed requests
// too, so retry past that.
const canaryUrl = `${config.baseUrl}canary.txt`;
// Wildcard resource, as clients sign (see sign.mjs --exact)
const params = signParams({ resource: `${canaryUrl}*`, ttl: 600, ...signer(config) });
let signed;
for (let attempt = 1; attempt <= 8; attempt++) {
  signed = await fetchRaw(`${canaryUrl}?${params}`);
  if (signed.status === 200) break;
  console.log(
    `Signed canary: ${signed.status} (x-cache: ${signed.headers["x-cache"] ?? "none"}), retrying in 10s (${attempt}/8)`,
  );
  await new Promise((resolve) => setTimeout(resolve, 10_000));
}
if (signed.status !== 200) {
  fail(
    `Signed canary still returns ${signed.status} after the error cache expired: the signature (or a web ACL) is rejecting it. Check that .work/keys/${config.key}.pem is the private half of the ${config.key} public key in Terraform, and that ${config.key} has key ID ${config.keyIds[config.key]} in a fresh outputs.json.`,
  );
}

// Only an explicit denial counts; errors and redirects prove nothing
const unsigned = await fetchRaw(canaryUrl);
if (![403, 404].includes(unsigned.status) || unsigned.body.includes("signed delivery canary")) {
  fail(
    `STOP: unsigned ${canaryUrl} returned ${unsigned.status}, not a 403/404 denial. Signed delivery may not be active on ${config.basePath}; nothing else was uploaded under it.`,
  );
}
console.log(`Canary: signed ${signed.status}, unsigned ${unsigned.status}`);

console.log(`Uploading ${basePrefix} families`);
copyTree(join(ASSETS, "restricted"), basePrefix);
console.log("Done");
