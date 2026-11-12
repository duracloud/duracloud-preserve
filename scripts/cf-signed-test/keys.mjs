// Generate an RSA 2048 key pair for signing: .work/keys/<name>.pem (private,
// never leaves .work) and <name>.pub.pem (goes into cloudfront_signed_public_keys).
// Usage: node keys.mjs [--name test_1]
import { generateKeyPairSync } from "node:crypto";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";
import { fail, KEYS } from "./lib.mjs";

const { values } = parseArgs({ options: { name: { type: "string", default: "test_1" } } });
const privatePath = join(KEYS, `${values.name}.pem`);
const publicPath = join(KEYS, `${values.name}.pub.pem`);
if (existsSync(privatePath)) fail(`${privatePath} already exists; delete it first to replace it`);

const { privateKey, publicKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
mkdirSync(KEYS, { recursive: true });
writeFileSync(privatePath, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });
writeFileSync(publicPath, publicKey.export({ type: "spki", format: "pem" }));

console.log(`Wrote ${privatePath}\nAdd this to cloudfront_signed_public_keys as ${values.name}:\n`);
console.log(publicKey.export({ type: "spki", format: "pem" }));
