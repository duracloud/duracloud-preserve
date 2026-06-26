// Build the native dcp binary and install it onto PATH.
// Usage: node scripts/install.mjs [--dir=<install-dir>]
//
// Builds a release binary for the host platform (so it runs locally — unlike the
// `cli` task, which cross-compiles a Linux binary for the Docker image), then
// copies it to the install dir. Defaults to ~/.local/bin, override with --dir or
// the DCP_INSTALL_DIR env var. The .exe suffix is added automatically on Windows.
import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import process from "node:process";
import { run } from "./lib.mjs";

const isWindows = process.platform === "win32";
const binName = isWindows ? "dcp.exe" : "dcp";

const dirArg = process.argv.slice(2).find((a) => a.startsWith("--dir="));
const installDir =
  dirArg?.slice("--dir=".length) || process.env.DCP_INSTALL_DIR || join(homedir(), ".local", "bin");

run("cargo", ["build", "--release", "--locked", "-p", "dcp", "--bin", "dcp"]);

const src = join("target", "release", binName);
const dest = join(installDir, binName);

mkdirSync(installDir, { recursive: true });
copyFileSync(src, dest);
if (!isWindows) chmodSync(dest, 0o755);

console.log(`Installed ${dest}`);
