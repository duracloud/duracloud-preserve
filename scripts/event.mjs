// Generate an event payload file from a function's sample event.
// Usage: node scripts/event.mjs --function=<name> --stack=<stack>
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { parseArgs } from "node:util";
import { fail, requireOpts } from "./lib.mjs";

const USAGE = "node scripts/event.mjs --function=<name> --stack=<stack>";

const { values } = parseArgs({
  options: {
    function: { type: "string", default: "" },
    stack: { type: "string", default: "" },
  },
});
requireOpts({ function: values.function, stack: values.stack }, USAGE);

const sample = `functions/${values.function}/events/sample.json`;
if (!existsSync(sample)) fail(`Error: ${sample} not found`);

mkdirSync("payloads", { recursive: true });
const out = `payloads/${values.function}.json`;
writeFileSync(out, readFileSync(sample, "utf8").replaceAll("test-stack", values.stack));
console.log(`Wrote ${out}`);
