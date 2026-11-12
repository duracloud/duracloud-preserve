// Shared helpers for the CloudFront signed delivery test harness (see README.md).
import { generateKeyPairSync } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { request } from "node:https";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { getSignedUrl } from "@aws-sdk/cloudfront-signer";
import { fail } from "../lib.mjs";

export { awsEnv, fail, run } from "../lib.mjs";

export const HERE = dirname(fileURLToPath(import.meta.url));
export const WORK = join(HERE, ".work");
export const KEYS = join(WORK, "keys");
export const ASSETS = join(WORK, "assets");

// Test families in their final key layout. Space "open-hls" is unsigned and
// lives at the bucket root; the others are under the signed base path.
export const FAMILIES = [
  { space: "open-hls", prefix: "TEST_O_0001" },
  { space: "campus-hls", prefix: "TEST_C_0001" },
  { space: "campus-hls", prefix: "TEST_C_0002" },
  { space: "closed-hls", prefix: "TEST_X_0001" },
  { space: "closed-hls", prefix: "TEST_X_0002" },
];

export function familyDir(space, prefix) {
  return space === "open-hls"
    ? join(ASSETS, "open-hls", prefix)
    : join(ASSETS, "restricted", space, prefix);
}

// Options shared by every command that talks to the stack
export const CONFIG_OPTIONS = {
  outputs: { type: "string", default: join(WORK, "outputs.json") },
  stack: { type: "string", default: "dcp-test" },
  key: { type: "string", default: "test_1" },
};

// Read `terraform output -json` from the deployment directory. Handles a
// stack module called directly (string/map values) or through for_each (values
// keyed by stack name), and PowerShell 5's UTF-16 redirection.
export function loadConfig({ outputs, stack, key }) {
  if (!existsSync(outputs)) {
    fail(
      `Missing ${outputs}\nIn your deployment directory run: terraform output -json > ${outputs}`,
    );
  }
  const raw = readFileSync(outputs);
  const text =
    raw[0] === 0xff && raw[1] === 0xfe
      ? raw.subarray(2).toString("utf16le")
      : raw.toString("utf8").replace(/^﻿/, "");
  const json = JSON.parse(text);

  const value = (name, isMap) => {
    const v = json[name]?.value;
    if (v === undefined || v === null) return null;
    if (typeof v === "string") return v;
    if (isMap && !(stack in v)) return v;
    return v[stack] ?? null;
  };

  const baseUrl = value("cloudfront_signed_base_url", false);
  if (!baseUrl) {
    fail(`cloudfront_signed_base_url is missing or null for ${stack} in ${outputs}`);
  }
  // Key IDs are a name => ID map, possibly nested under the stack name
  const rawKeyIds = json.cloudfront_signed_key_ids?.value ?? {};
  const keyIds =
    typeof rawKeyIds[stack] === "object" && rawKeyIds[stack] !== null
      ? rawKeyIds[stack]
      : rawKeyIds;

  const url = new URL(baseUrl);
  return {
    stack,
    bucket: `${stack}-public`,
    baseUrl,
    basePath: url.pathname,
    host: url.host,
    origin: url.origin,
    cloudfrontDomain: value("cloudfront_domain_name", false),
    keyIds,
    key,
  };
}

// The private key and registered key ID for a key name (e.g. test_1)
export function signer(config, name = config.key) {
  const keyPairId = config.keyIds[name];
  if (!keyPairId) {
    fail(`No key ID for ${name} in outputs (have: ${Object.keys(config.keyIds).join(", ")})`);
  }
  return { keyPairId, privateKey: readPrivateKey(name) };
}

export function readPrivateKey(name) {
  const path = join(KEYS, `${name}.pem`);
  if (!existsSync(path)) fail(`Missing private key ${path} (run: node keys.mjs --name ${name})`);
  return readFileSync(path, "utf8");
}

// An RSA key CloudFront has never seen, for "unregistered key" checks
export function throwawayKey() {
  return generateKeyPairSync("rsa", { modulusLength: 2048 }).privateKey.export({
    type: "pkcs8",
    format: "pem",
  });
}

// Sign a custom policy for `resource` (normally "<directory URL>*") and return
// the query string to append to any URL it covers. This is what the client does.
export function signParams({ resource, keyPairId, privateKey, ttl = 300, algorithm = "SHA256" }) {
  const policy = JSON.stringify({
    Statement: [
      {
        Resource: resource,
        Condition: { DateLessThan: { "AWS:EpochTime": Math.floor(Date.now() / 1000) + ttl } },
      },
    ],
  });
  const signed = getSignedUrl({
    url: resource.replace(/\*$/, ""),
    policy,
    keyPairId,
    privateKey,
    algorithm,
  });
  return new URL(signed).search.slice(1);
}

// One HTTPS request with no redirects, decompression, or cookies. Node sends
// no User-Agent by default, and WAF managed rules (NoUserAgent_HEADER) block
// that, which looks exactly like a denied signature behind the 403 -> 404 mapping.
export function fetchRaw(url, { method = "GET", headers = {} } = {}) {
  return new Promise((resolve, reject) => {
    const allHeaders = { "User-Agent": "dcp-cf-signed-test", ...headers };
    const req = request(url, { method, headers: allHeaders }, (res) => {
      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () =>
        resolve({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks) }),
      );
    });
    req.on("error", reject);
    req.end();
  });
}

export function contentType(file) {
  if (file.endsWith(".m3u8")) return "application/vnd.apple.mpegurl";
  if (file.endsWith(".ts")) return "video/mp2t";
  return "text/plain";
}
