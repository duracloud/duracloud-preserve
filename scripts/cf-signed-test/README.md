# CloudFront signed delivery test harness

End-to-end QA for signed delivery and per-path CORS on a deployed stack, normally `dcp-test`. The harness plays the client: it holds the signing keys, generates synthetic HLS content, uploads it, runs HTTP checks, and serves a fake client app with a player. Unit tests for the Terraform and response function live in `terraform/modules/stack/tests/`. For how signed delivery works and the client contract, see [Signed delivery](../../docs/src/technical/signed-delivery.md).

| Script | Purpose |
| --- | --- |
| `keys.mjs` | Generate a signing key pair |
| `assets.mjs` | Generate the test families with ffmpeg |
| `upload.mjs` | Upload them, gated on a canary that must be denied unsigned and served signed |
| `checks.mjs` | HTTP check matrix; `--keys` for key rotation |
| `client.mjs` | Fake client app and player page |
| `sign.mjs` | Print a signed URL, for manual checks |

Generated files (keys, assets, outputs) go in `.work/`, which is gitignored. Private keys never leave it.

Run the scripts from this directory with `node` directly. `npm run` breaks Ctrl+C, especially on Windows.

## Setup

Requires Node 24, the AWS CLI with a profile for the stack's account, and `ffmpeg` on `PATH` (Windows: `winget install ffmpeg`).

```bash
npm ci
node keys.mjs --name test_1     # or copy an existing private key to .work/keys/test_1.pem
```

Before the first apply, list the public bucket under `restricted/`: anything there stops being public. Then set these on the stack and apply:

```hcl
cloudfront_signed_enabled     = true
cloudfront_signed_public_keys = { test_1 = "<contents of .work/keys/test_1.pub.pem>" }
cloudfront_cors_allowed_origins_by_path = {
  "/open-hls/"   = ["http://localhost:8080", "http://localhost:8081"]
  "/restricted/" = ["http://localhost:8080"]
}
```

`:8081` is allowed on `/open-hls/` only, so the checks can show each path gets only its own origins.

The deployment config must expose `cloudfront_signed_base_url` and `cloudfront_signed_key_ids`; `cloudfront_domain_name` is optional and enables check R17. Save the outputs and generate the assets:

```bash
terraform output -json > <repo>/scripts/cf-signed-test/.work/outputs.json   # in the deployment directory
node assets.mjs                                                              # about 40 MB
```

Scripts that talk to the stack take `--stack` (default `dcp-test`), `--key` (default `test_1`), and `--outputs` (default `.work/outputs.json`).

## Test assets

Two-minute families in their final key layout, using the client's file names (`<prefix>-playlist.m3u8`, `<prefix>-a160k_v4.m3u8`, `<prefix>-a160k_v4.ts`):

| Space | Prefix | Shape |
| --- | --- | --- |
| `open-hls` | `TEST_O_0001` | single file + byte ranges |
| `campus-hls` | `TEST_C_0001` | single file + byte ranges |
| `campus-hls` | `TEST_C_0002` | segmented, 2 quality levels |
| `closed-hls` | `TEST_X_0001` | single file + byte ranges |
| `closed-hls` | `TEST_X_0002` | audio only, 7 s byte-range segments (matches a real `closed-hls` sample) |

## Automated checks

```bash
node upload.mjs --profile <aws-profile>
node checks.mjs                  # about 30 s; exits non-zero on failure
node checks.mjs --only R4,R5b    # subset
```

`<R>` is `restricted/campus-hls/TEST_C_0001/` and `<O>` is `open-hls/TEST_O_0001/`. "Denied" means 403 or 404 with no content.

| # | Check | Expect |
| --- | --- | --- |
| R1 | `<R>` master, unsigned | denied |
| R2 | `<R>` master, signed | 200 |
| R3 | Every file in `<R>` with one signature | 200 |
| R4 | `<R>` `.ts`, signed, `Range`, `Origin: http://localhost:8080` | 206 with `Content-Range`, ACAO for the origin, `Content-Range` exposed, `Vary: Origin`, function headers, no credentials |
| R5 | R4 with `Origin: https://evil.example` | 206, no CORS headers, `Vary: Origin` |
| R5a | R4 with `Origin: http://localhost:8081` | 206, no ACAO |
| R5b | R4's object as a cache hit, with origins `:8080`, evil, none, `:8080` | ACAO `:8080`, none, none, `:8080` |
| R5c | Bad signature, and a missing object, with an allowed origin | denied, no CORS headers |
| R6 | One character of `Signature` changed | denied |
| R7 | `Policy` swapped for one covering the whole base path | denied |
| R8 | 5 s TTL, requested after 10 s | denied |
| R9 | `TEST_C_0002` with `<R>`'s signature | denied |
| R10 | Policy `…/TEST_C_000*` (no `/` before `*`) | both `TEST_C_0001` and `TEST_C_0002` 200: why the `/` matters |
| R11 | Cache warmed by a signed request, then unsigned | denied |
| R12 | Unsigned range request, then signed | denied, then 206 (no cached error) |
| R13 | `closed-hls` families, own signatures | 200 |
| R14 | Unregistered key ID, validly signed with its own key | denied |
| R15 | Anonymous GET from S3 | 403 |
| R16 | Signed master | HLS `content-type`, `nosniff`, HLS CSP |
| R17 | `*.cloudfront.net` host: unsigned; signed for the custom host; signed for its own host | denied, denied, 200 |
| R18 | Unsigned `OPTIONS` preflight | denied, no ACAO |
| O1 | `<O>` master, unsigned | 200 |
| O2 | `<O>` `.ts`, `Range`, `Origin: http://localhost:8080` | 206 + ACAO |
| O2a | O2 with `Origin: http://localhost:8081` | 206 + ACAO |
| O3 | `/404.txt` with an allowed origin | no CORS headers, no `Vary: Origin` |

## Browser checks

```bash
node client.mjs                  # Ctrl+C to stop
```

Open <http://localhost:8080> (not `127.0.0.1`, a different origin), pick a resource, and press **New session and play** or click the video. The log under the video shows each step. Test Chrome and Firefox, Safari with **Native HLS** on and off, and iOS Safari if available.

- [ ] Every resource plays to the end.
- [ ] Seek both ways; pause and resume.
- [ ] `TEST_C_0002` switches quality levels.
- [ ] DevTools shows no CORS errors and no `OPTIONS` requests.
- [ ] Expiry: set TTL to 60, play a video family, pause past expiry, then seek ahead. The page shows an error (hls.js: network error with no readable status; native Safari: 404), and a new session plays again. The audio-only family buffers completely, so it may never hit expiry.
- [ ] A second session for the same resource is a cache hit (`x-cache: Hit from cloudfront`).
- [ ] `node client.mjs --port 8081`: `open-hls` plays, restricted families fail with CORS errors.

## Key rotation

`node checks.mjs --keys` expects every registered key with a private key in `.work/keys/` to sign successfully. Add `--revoked <name>=<old key ID>` (repeatable) to expect a removed key to be denied. Refresh `outputs.json` after each apply.

| # | Step | Expect |
| --- | --- | --- |
| K1 | `node keys.mjs --name test_2`, add it to the stack, apply, `--keys` | both keys 200 |
| K2 | Remove `test_1`, apply, wait for deployment, `--keys --revoked test_1=<old ID>` | `test_1` denied, `test_2` 200. Record the deployment time: that's how long revoking a compromised key takes |
| K3 | Rename `.work/keys/test_2.pem` to `test_2_old.pem`, generate a new `test_2`, replace its PEM in the stack, apply, `--keys --revoked test_2_old=<old ID>` | apply succeeds; new key 200, old key denied |

## Findings from `dcp-test`

- Every denial (missing object, bad or expired signature, preflight, WAF block) reaches the viewer as 404.
- The WAF blocks requests without a `User-Agent`, which looks like a bad signature. Client apps that fetch playlists server-side must send one.
- A 404 for a missing object is cached for 60 s, including for signed requests.

## Clean up

Done when all automated, browser, and key rotation checks pass. Delete the test objects **before** disabling signed delivery, or they become public:

```bash
aws s3 rm s3://dcp-test-public/restricted/ --recursive --profile <aws-profile>
aws s3 rm s3://dcp-test-public/open-hls/ --recursive --profile <aws-profile>
```

Then remove the test keys and CORS paths from the stack, apply, and delete `.work/`.
