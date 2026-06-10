// Run a command with environment variables set — replaces shell `VAR=x cmd` syntax.
// Usage: node scripts/env-run.mjs [--profile=<aws-profile>] [--set NAME=VALUE]... -- <command> [args...]
//
// --profile sets AWS_PROFILE. Empty values (--profile=, --set NAME=) are skipped
// so unset mise task options fall through to the ambient environment.
import { fail, run } from "./lib.mjs";

const USAGE =
  "Usage: node scripts/env-run.mjs [--profile=<aws-profile>] [--set NAME=VALUE]... -- <command> [args...]";

const argv = process.argv.slice(2);
const sep = argv.indexOf("--");
if (sep === -1 || sep === argv.length - 1) fail(USAGE);

const env = {};
for (let i = 0; i < sep; i++) {
  const arg = argv[i];
  if (arg.startsWith("--profile=")) {
    const profile = arg.slice("--profile=".length);
    if (profile) env.AWS_PROFILE = profile;
  } else if (arg === "--set") {
    const pair = argv[++i] ?? "";
    const eq = pair.indexOf("=");
    if (eq < 1) fail(`Invalid --set value '${pair}' (expected NAME=VALUE)\n${USAGE}`);
    const value = pair.slice(eq + 1);
    if (value) env[pair.slice(0, eq)] = value;
  } else {
    fail(`Unknown argument before --: ${arg}\n${USAGE}`);
  }
}

const [cmd, ...args] = argv.slice(sep + 1);
run(cmd, args, { env });
