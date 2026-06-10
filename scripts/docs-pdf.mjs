// Build the docs PDF with WeasyPrint.
// Usage: node scripts/docs-pdf.mjs [--output=<file>]
import { spawnSync } from "node:child_process";
import { parseArgs } from "node:util";
import { fail, run } from "./lib.mjs";

const { values } = parseArgs({
  options: { output: { type: "string", default: "" } },
});
const output = values.output || "duracloud-preserve.pdf";

const check = spawnSync("weasyprint", ["--version"], { stdio: "ignore" });
if (check.error) fail("WeasyPrint is required: https://weasyprint.org/");

run("mdbook", ["build"], { cwd: "docs" });
run("weasyprint", ["docs/book/print.html", output]);
console.log(`Wrote ${output}`);
