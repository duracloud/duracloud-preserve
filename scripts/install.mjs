// Build the native dcp binary and install it onto PATH.
// Usage: node scripts/install.mjs [--dir=<install-dir>]
//
// Builds a release binary for the host platform (so it runs locally — unlike the
// `cli` task, which cross-compiles a Linux binary for the Docker image), then
// copies it to the install dir. Defaults to ~/.local/bin, override with --dir or
// the DCP_INSTALL_DIR env var. The .exe suffix is added automatically on Windows.
//
// dcp dynamically links the prebuilt libduckdb, so the install ships the shared
// library alongside the binary. Windows resolves DLLs from the exe's directory;
// on Linux/macOS the install build adds an rpath pointing at the binary's own
// directory ($ORIGIN / @loader_path) so the installed pair is self-contained —
// without it the binary would only find the library inside this repo's target/.
import { chmodSync, copyFileSync, existsSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import process from "node:process";
import { fail, run } from "./lib.mjs";

const isWindows = process.platform === "win32";
const binName = isWindows ? "dcp.exe" : "dcp";
const libName = isWindows
  ? "duckdb.dll"
  : process.platform === "darwin"
    ? "libduckdb.dylib"
    : "libduckdb.so";

const dirArg = process.argv.slice(2).find((a) => a.startsWith("--dir="));
const installDir =
  dirArg?.slice("--dir=".length) || process.env.DCP_INSTALL_DIR || join(homedir(), ".local", "bin");

// RUSTFLAGS forces a rebuild relative to plain release builds, but installs are
// occasional. Not set on Windows: it would override the [target.*] rustflags in
// .cargo/config.toml, and DLL lookup needs no rpath anyway.
const origin = process.platform === "darwin" ? "@loader_path" : "$ORIGIN";
const env = isWindows ? undefined : { RUSTFLAGS: `-C link-arg=-Wl,-rpath,${origin}` };

run("cargo", ["build", "--release", "--locked", "-p", "dcp", "--bin", "dcp"], { env });

const src = join("target", "release", binName);
const dest = join(installDir, binName);

mkdirSync(installDir, { recursive: true });
copyFileSync(src, dest);
if (!isWindows) chmodSync(dest, 0o755);

// libduckdb-sys copies the shared library into target/release/deps during the build.
const lib = join("target", "release", "deps", libName);
if (!existsSync(lib)) fail(`Missing ${lib} — expected the build to place it there`);
copyFileSync(lib, join(installDir, libName));
console.log(`Installed ${join(installDir, libName)}`);

console.log(`Installed ${dest}`);
