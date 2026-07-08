// Stage the prebuilt libduckdb.so for linux-arm64 deploy artifacts.
//
// The build (libduckdb-sys with DUCKDB_DOWNLOAD_LIB=1) downloads the shared
// library into target/duckdb-download/<target>/<version>/. Deploy packaging
// needs it at a stable path: `lib/libduckdb.so` is included in the duckdb
// lambda zips (Lambda searches /var/task/lib) and copied into the dcp Docker
// image. The version is derived from libduckdb-sys in Cargo.lock so the
// staged library always matches what the binary was linked against.
import { copyFileSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fail } from "./lib.mjs";

// libduckdb-sys encodes the DuckDB version as 1.MAJOR_MINOR_PATCH.x,
// e.g. 1.10504.0 -> DuckDB 1.5.4.
function duckdbVersion() {
  const lock = readFileSync("Cargo.lock", "utf8");
  const match = lock.match(/name = "libduckdb-sys"\r?\nversion = "1\.(\d+)\./);
  if (!match) fail("libduckdb-sys not found in Cargo.lock");
  const encoded = Number(match[1]);
  return `${Math.floor(encoded / 10_000)}.${Math.floor(encoded / 100) % 100}.${encoded % 100}`;
}

const version = duckdbVersion();
const source = join(
  "target",
  "duckdb-download",
  "aarch64-unknown-linux-gnu",
  version,
  "libduckdb.so",
);

mkdirSync("lib", { recursive: true });
try {
  copyFileSync(source, join("lib", "libduckdb.so"));
} catch {
  fail(`Missing ${source} — run an arm64 build first (mise run build or cli)`);
}
console.log(`Staged ${source} -> lib/libduckdb.so`);
