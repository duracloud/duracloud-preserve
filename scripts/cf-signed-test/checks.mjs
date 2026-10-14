// HTTP check matrix for signed delivery and per-path CORS (harness section 5).
// Needs the assets uploaded (upload.mjs). Exits non-zero if any check fails.
// Usage: node checks.mjs [--only R4,R5] [--key test_1] [--stack dcp-test]
//        node checks.mjs --keys [--revoked test_1=K2ABC...]   (key rotation, K1-K3)
import { readdirSync } from "node:fs";
import { parseArgs } from "node:util";
import {
  CONFIG_OPTIONS,
  familyDir,
  fetchRaw,
  loadConfig,
  readPrivateKey,
  signer,
  signParams,
  throwawayKey,
} from "./lib.mjs";

const { values } = parseArgs({
  options: {
    ...CONFIG_OPTIONS,
    only: { type: "string", default: "" },
    keys: { type: "boolean", default: false },
    revoked: { type: "string", multiple: true, default: [] },
  },
});
const config = loadConfig(values);
const only = new Set(values.only.split(",").filter(Boolean));

const ALLOWED = "http://localhost:8080";
const OPEN_ONLY = "http://localhost:8081";
const EVIL = "https://evil.example";

// <R>, <O>, and the other directories the checks use
const dirUrl = (space, prefix, host = config.host) =>
  `https://${host}${space === "open-hls" ? "/open-hls/" : `${config.basePath}${space}/`}${prefix}/`;
const R = dirUrl("campus-hls", "TEST_C_0001");
const O = dirUrl("open-hls", "TEST_O_0001");
const master = (prefix) => `${prefix}-playlist.m3u8`;
const singleTs = (prefix) => `${prefix}-a160k_v4.ts`;

// Explicit credentials skip the default key, which may be gone after rotation
const sig = (dir, opts = {}) =>
  signParams({
    resource: `${dir}*`,
    ...(opts.keyPairId && opts.privateKey ? {} : signer(config)),
    ...opts,
  });
const get = (url, headers = {}, method = "GET") => fetchRaw(url, { method, headers });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const isPlaylist = (res) => res.body.subarray(0, 7).toString() === "#EXTM3U";
const corsNames = (res) => Object.keys(res.headers).filter((h) => h.startsWith("access-control-"));
const varies = (res) =>
  (res.headers.vary ?? "")
    .split(",")
    .map((v) => v.trim().toLowerCase())
    .includes("origin");
const isHit = (res) => /^hit from cloudfront$/i.test(res.headers["x-cache"] ?? "");

// Denied by CloudFront (signature) or the 403 -> /404.txt mapping
const deniedStatuses = [];
function expectDenied(res, label) {
  deniedStatuses.push(`${label}: ${res.status}`);
  expect([403, 404].includes(res.status), `${label}: expected 403/404, got ${res.status}`);
  expect(!isPlaylist(res), `${label}: denied response contains playlist content`);
}

function expect(condition, message) {
  if (!condition) throw new Error(message);
}

function expectStatus(res, status, label) {
  expect(res.status === status, `${label}: expected ${status}, got ${res.status}`);
}

// CloudFront URL-safe base64, for tampering with the policy
const safe64 = (text) =>
  Buffer.from(text).toString("base64").replace(/\+/g, "-").replace(/=/g, "_").replace(/\//g, "~");

async function warm(url, headers = {}) {
  for (let i = 0; i < 6; i++) {
    const res = await get(url, headers);
    if (isHit(res)) return res;
    await sleep(500);
  }
  throw new Error(`never became a cache hit: ${url.split("?")[0]}`);
}

const results = [];
const skipped = [];
async function check(id, description, fn) {
  if (only.size && !only.has(id)) return;
  try {
    const note = await fn();
    if (note?.startsWith("skipped:")) {
      skipped.push(id);
      console.log(`SKIP ${id} ${description} (${note.slice(9)})`);
      return;
    }
    results.push({ id, ok: true });
    console.log(`PASS ${id} ${description}${note ? ` (${note})` : ""}`);
  } catch (error) {
    results.push({ id, ok: false });
    console.log(`FAIL ${id} ${description}\n     ${error.message}`);
  }
}

async function matrix() {
  const sigR = sig(R);

  await check("R1", "unsigned master is denied", async () => {
    const res = await get(`${R}${master("TEST_C_0001")}`);
    expectDenied(res, "R1");
    return `status ${res.status}`;
  });

  await check("R2", "signed master is served", async () => {
    const res = await get(`${R}${master("TEST_C_0001")}?${sigR}`);
    expectStatus(res, 200, "R2");
    expect(isPlaylist(res), "R2: not a playlist");
  });

  await check("R3", "one signature covers every file in the directory", async () => {
    const files = readdirSync(familyDir("campus-hls", "TEST_C_0001"));
    for (const file of files) {
      expectStatus(await get(`${R}${file}?${sigR}`, {}, "HEAD"), 200, file);
    }
    return `${files.length} files`;
  });

  const tsUrl = `${R}${singleTs("TEST_C_0001")}?${sigR}`;
  const ranged = (origin) =>
    get(tsUrl, { Range: "bytes=0-999", ...(origin && { Origin: origin }) });

  await check(
    "R4",
    "range request with allowed origin gets 206, CORS, and function headers",
    async () => {
      const res = await ranged(ALLOWED);
      expectStatus(res, 206, "R4");
      expect(res.headers["content-range"]?.startsWith("bytes 0-999/"), "R4: no Content-Range");
      expect(res.headers["access-control-allow-origin"] === ALLOWED, "R4: wrong or missing ACAO");
      expect(
        /content-range/i.test(res.headers["access-control-expose-headers"] ?? ""),
        "R4: Content-Range not exposed",
      );
      expect(varies(res), "R4: Vary lacks Origin");
      expect(res.headers["content-type"] === "video/mp2t", "R4: wrong content-type");
      expect(res.headers["x-content-type-options"] === "nosniff", "R4: no nosniff");
      expect(res.headers["content-security-policy"], "R4: no CSP");
      expect(!res.headers["access-control-allow-credentials"], "R4: credentials allowed");
    },
  );

  await check("R5", "other origin gets no CORS headers, Vary still has Origin", async () => {
    const res = await ranged(EVIL);
    expectStatus(res, 206, "R5");
    expect(corsNames(res).length === 0, `R5: got ${corsNames(res)}`);
    expect(varies(res), "R5: Vary lacks Origin");
  });

  await check("R5a", "origin allowed only on /open-hls/ gets nothing here", async () => {
    const res = await ranged(OPEN_ONLY);
    expectStatus(res, 206, "R5a");
    expect(corsNames(res).length === 0, `R5a: got ${corsNames(res)}`);
  });

  await check("R5b", "cache hits keep CORS specific to each request", async () => {
    await warm(tsUrl, { Range: "bytes=0-999", Origin: ALLOWED });
    for (const [origin, allowed] of [
      [ALLOWED, ALLOWED],
      [EVIL, undefined],
      [undefined, undefined],
      [ALLOWED, ALLOWED],
    ]) {
      const res = await ranged(origin);
      expect(isHit(res), `R5b: ${origin ?? "no origin"} was not a cache hit`);
      expect(
        res.headers["access-control-allow-origin"] === allowed,
        `R5b: ${origin ?? "no origin"} got ACAO ${res.headers["access-control-allow-origin"]}`,
      );
      expect(varies(res), `R5b: ${origin ?? "no origin"} Vary lacks Origin`);
    }
  });

  await check("R5c", "denied and missing responses carry no CORS headers", async () => {
    const tampered = await get(`${R}${master("TEST_C_0001")}?${tamperSignature(sigR)}`, {
      Origin: ALLOWED,
    });
    expectDenied(tampered, "R5c tampered");
    expect(corsNames(tampered).length === 0, `R5c tampered: got ${corsNames(tampered)}`);
    const missing = await get(`${R}nope.ts?${sigR}`, { Origin: ALLOWED });
    expectDenied(missing, "R5c missing");
    expect(corsNames(missing).length === 0, `R5c missing: got ${corsNames(missing)}`);
    return `tampered ${tampered.status}, missing ${missing.status}`;
  });

  await check("R6", "tampered signature is denied", async () => {
    expectDenied(await get(`${R}${master("TEST_C_0001")}?${tamperSignature(sigR)}`), "R6");
  });

  await check("R7", "policy swapped for one granting the whole base path is denied", async () => {
    const params = new URLSearchParams(sigR);
    const exp = Math.floor(Date.now() / 1000) + 300;
    params.set(
      "Policy",
      safe64(
        JSON.stringify({
          Statement: [
            {
              Resource: `https://${config.host}${config.basePath}*`,
              Condition: { DateLessThan: { "AWS:EpochTime": exp } },
            },
          ],
        }),
      ),
    );
    expectDenied(await get(`${R}${master("TEST_C_0001")}?${params}`), "R7");
  });

  await check("R8", "expired signature is denied (waits 10s)", async () => {
    const short = sig(R, { ttl: 5 });
    await sleep(10_000);
    expectDenied(await get(`${R}${master("TEST_C_0001")}?${short}`), "R8");
  });

  await check("R9", "signature for one directory is denied on another", async () => {
    const other = dirUrl("campus-hls", "TEST_C_0002");
    expectDenied(await get(`${other}${master("TEST_C_0002")}?${sigR}`), "R9");
  });

  await check(
    "R10",
    "policy without '/' before '*' matches sibling directories (pitfall)",
    async () => {
      const loose = signParams({
        resource: `https://${config.host}${config.basePath}campus-hls/TEST_C_000*`,
        ...signer(config),
      });
      for (const prefix of ["TEST_C_0001", "TEST_C_0002"]) {
        const res = await get(`${dirUrl("campus-hls", prefix)}${master(prefix)}?${loose}`);
        expectStatus(res, 200, `R10 ${prefix}`);
      }
    },
  );

  await check("R11", "unsigned request after warming the cache is denied", async () => {
    await warm(`${R}${master("TEST_C_0001")}?${sigR}`);
    expectDenied(await get(`${R}${master("TEST_C_0001")}`), "R11");
  });

  await check("R12", "denied request doesn't cache an error for the signed retry", async () => {
    const url = `${R}${singleTs("TEST_C_0001")}`;
    expectDenied(await get(url, { Range: "bytes=1000-1999" }), "R12 unsigned");
    expectStatus(await get(`${url}?${sigR}`, { Range: "bytes=1000-1999" }), 206, "R12 signed");
  });

  await check("R13", "closed-hls families work with their own signatures", async () => {
    for (const prefix of ["TEST_X_0001", "TEST_X_0002"]) {
      const dir = dirUrl("closed-hls", prefix);
      const res = await get(`${dir}${master(prefix)}?${sig(dir)}`);
      expectStatus(res, 200, `R13 ${prefix}`);
      expect(isPlaylist(res), `R13 ${prefix}: not a playlist`);
    }
  });

  await check("R14", "unregistered key ID with its own valid signature is denied", async () => {
    const params = sig(R, { keyPairId: "KUNREGISTERED0", privateKey: throwawayKey() });
    expectDenied(await get(`${R}${master("TEST_C_0001")}?${params}`), "R14");
  });

  await check("R15", "direct S3 access is denied", async () => {
    // The global endpoint redirects to the bucket's region and names it
    const key = `${new URL(R).pathname}${master("TEST_C_0001")}`;
    let res = await get(`https://${config.bucket}.s3.amazonaws.com${key}`);
    const region = res.headers["x-amz-bucket-region"];
    if (res.status !== 403 && region) {
      res = await get(`https://${config.bucket}.s3.${region}.amazonaws.com${key}`);
    }
    expectStatus(res, 403, "R15");
    expect(!isPlaylist(res), "R15: S3 returned playlist content");
    return region;
  });

  await check("R16", "function headers on signed responses", async () => {
    const res = await get(`${R}${master("TEST_C_0001")}?${sigR}`);
    expect(res.headers["content-type"] === "application/vnd.apple.mpegurl", "R16: content-type");
    expect(res.headers["x-content-type-options"] === "nosniff", "R16: nosniff");
    expect(res.headers["content-security-policy"]?.includes("connect-src 'self'"), "R16: HLS CSP");
  });

  await check("R17", "*.cloudfront.net host is protected too", async () => {
    if (!config.cloudfrontDomain) return "skipped: no cloudfront_domain_name output";
    const cf = dirUrl("campus-hls", "TEST_C_0001", config.cloudfrontDomain);
    expectDenied(await get(`${cf}${master("TEST_C_0001")}`), "R17 unsigned");
    expectDenied(await get(`${cf}${master("TEST_C_0001")}?${sigR}`), "R17 other host's policy");
    expectStatus(await get(`${cf}${master("TEST_C_0001")}?${sig(cf)}`), 200, "R17 own policy");
  });

  await check("R18", "unsigned preflight is denied", async () => {
    const res = await get(
      `${R}${master("TEST_C_0001")}`,
      { Origin: ALLOWED, "Access-Control-Request-Method": "GET" },
      "OPTIONS",
    );
    expectDenied(res, "R18");
    expect(!res.headers["access-control-allow-origin"], "R18: preflight got ACAO");
    return `status ${res.status}`;
  });

  await check("O1", "open master is served unsigned", async () => {
    const res = await get(`${O}${master("TEST_O_0001")}`);
    expectStatus(res, 200, "O1");
    expect(isPlaylist(res), "O1: not a playlist");
  });

  for (const [id, origin] of [
    ["O2", ALLOWED],
    ["O2a", OPEN_ONLY],
  ]) {
    await check(id, `open range request with ${origin} gets CORS`, async () => {
      const res = await get(`${O}${singleTs("TEST_O_0001")}`, {
        Range: "bytes=0-999",
        Origin: origin,
      });
      expectStatus(res, 206, id);
      expect(res.headers["access-control-allow-origin"] === origin, `${id}: wrong or missing ACAO`);
    });
  }

  await check("O3", "unlisted paths get no CORS", async () => {
    const res = await get(`${config.origin}/404.txt`, { Origin: ALLOWED });
    expect(corsNames(res).length === 0, `O3: got ${corsNames(res)}`);
    expect(!varies(res), "O3: Vary has Origin");
  });
}

function tamperSignature(params) {
  const p = new URLSearchParams(params);
  const s = p.get("Signature");
  const i = Math.floor(s.length / 2);
  p.set("Signature", `${s.slice(0, i)}${s[i] === "A" ? "B" : "A"}${s.slice(i + 1)}`);
  return p.toString();
}

// K1-K3: every registered key with a local private key signs successfully;
// each --revoked name=keyId (an old ID with its old private key) is denied
async function keys() {
  for (const [name, keyPairId] of Object.entries(config.keyIds)) {
    await check(`K ${name}`, `registered key ${name} (${keyPairId}) signs`, async () => {
      const params = sig(R, { keyPairId, privateKey: readPrivateKey(name) });
      expectStatus(await get(`${R}${master("TEST_C_0001")}?${params}`), 200, name);
    });
  }
  for (const entry of values.revoked) {
    const [name, keyPairId] = entry.split("=");
    await check(`K ${name}`, `revoked key ${name} (${keyPairId}) is denied`, async () => {
      const params = sig(R, { keyPairId, privateKey: readPrivateKey(name) });
      expectDenied(await get(`${R}${master("TEST_C_0001")}?${params}`), name);
    });
  }
}

console.log(`Checking ${config.baseUrl} (${config.stack}, key ${config.key})\n`);
await (values.keys ? keys() : matrix());

const failed = results.filter((r) => !r.ok);
console.log(
  `\n${results.length - failed.length} passed, ${failed.length} failed` +
    (skipped.length ? `, ${skipped.length} skipped (${skipped.join(", ")})` : ""),
);
if (deniedStatuses.length) console.log(`Denied statuses: ${deniedStatuses.join(", ")}`);
process.exit(failed.length ? 1 : 0);
