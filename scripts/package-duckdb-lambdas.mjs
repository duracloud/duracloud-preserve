// Re-package the duckdb-linking lambda zips with the prebuilt libduckdb.so.
//
// cargo-lambda's packaging phase zips EVERY workspace binary it finds, even
// when scoped with -p — so passing --include on a re-zip pass would add the
// shared library to all seven function zips instead of just the two that link
// duckdb. To keep the other zips lean, the --include pass writes into a
// separate --lambda-dir and only the duckdb functions' zips are promoted over
// the real ones in target/lambda/.
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { run } from "./lib.mjs";

const DUCKDB_FUNCTIONS = ["checksum-report", "inventory-report"];
const STAGING = join("target", "lambda-duckdb");

run("node", [join("scripts", "stage-duckdb-lib.mjs")]);

run("cargo", [
  "lambda",
  "build",
  ...DUCKDB_FUNCTIONS.flatMap((f) => ["-p", f]),
  "--release",
  "--arm64",
  "--output-format",
  "zip",
  "--include",
  "lib/libduckdb.so",
  "--lambda-dir",
  STAGING,
]);

for (const fn of DUCKDB_FUNCTIONS) {
  const src = join(STAGING, fn, "bootstrap.zip");
  const dest = join("target", "lambda", fn, "bootstrap.zip");
  mkdirSync(dirname(dest), { recursive: true });
  copyFileSync(src, dest);
  console.log(`Packaged ${dest} (with lib/libduckdb.so)`);
}

rmSync(STAGING, { recursive: true, force: true });
