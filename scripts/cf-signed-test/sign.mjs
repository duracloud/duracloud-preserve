// Print a signed URL (or just the parameters) for one directory under the
// signed base path, the way the client signs: one custom policy per directory.
// Usage: node sign.mjs [--dir campus-hls/TEST_C_0001/] [--file TEST_C_0001-playlist.m3u8]
//          [--ttl 300] [--host <host>] [--algorithm SHA256|SHA1] [--key test_1] [--stack dcp-test]
//          [--outputs path] [--exact]
// With no --dir, the policy covers the whole base path (diagnostics only).
// --exact signs the file URL itself instead of the directory wildcard.
import { parseArgs } from "node:util";
import { CONFIG_OPTIONS, fail, loadConfig, signer, signParams } from "./lib.mjs";

const { values } = parseArgs({
  options: {
    ...CONFIG_OPTIONS,
    dir: { type: "string", default: "" },
    file: { type: "string", default: "" },
    ttl: { type: "string", default: "300" },
    host: { type: "string", default: "" },
    algorithm: { type: "string", default: "SHA256" },
    exact: { type: "boolean", default: false },
  },
});
const config = loadConfig(values);
if (values.exact && !values.file) fail("--exact needs --file");
if (!/^([A-Za-z0-9_-]+\/)*$/.test(values.dir)) {
  fail("--dir must be a directory under the base path ending in '/', e.g. campus-hls/TEST_C_0001/");
}

const dirUrl = `https://${values.host || config.host}${config.basePath}${values.dir}`;
const params = signParams({
  resource: values.exact ? `${dirUrl}${values.file}` : `${dirUrl}*`,
  ttl: Number(values.ttl),
  algorithm: values.algorithm,
  ...signer(config),
});
console.log(values.file ? `${dirUrl}${values.file}?${params}` : params);
