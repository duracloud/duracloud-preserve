// Generate Terraform _locals.tf files from shared/constants/src/lib.rs.
// Rust is the canonical source for shared constants — do not edit _locals.tf by hand.
import { readFileSync, writeFileSync } from "node:fs";
import { capture } from "./lib.mjs";

const RUST_SRC = "shared/constants/src/lib.rs";
const TF_OUT = [
  "terraform/modules/archive_it/_locals.tf",
  "terraform/modules/stack/_locals.tf",
  "terraform/modules/users/_locals.tf",
];

const HEADER = `# Generated from shared/constants/src/lib.rs — do not edit.
# Run \`mise run locals\` to regenerate.

locals {
`;

let body = "";
for (const line of readFileSync(RUST_SRC, "utf8").split(/\r?\n/)) {
  // Pass through section comments
  const comment = line.match(/^\/\/ (.+)/);
  if (comment) {
    body += `\n  # ${comment[1]}\n`;
    continue;
  }

  // pub const NAME: &str = "value";
  const str = line.match(/^pub const ([A-Z_]+): &str = "([^"]*)";/);
  if (str) {
    body += `  ${str[1].toLowerCase().padEnd(45)} = "${str[2]}"\n`;
    continue;
  }

  // pub const NAME: <integer> = <number>;
  const num = line.match(
    /^pub const ([A-Z_]+): (?:u8|u16|u32|u64|usize|i8|i16|i32|i64) = ([0-9]+);/,
  );
  if (num) {
    body += `  ${num[1].toLowerCase().padEnd(45)} = ${num[2]}\n`;
  }
}

for (const out of TF_OUT) writeFileSync(out, `${HEADER}${body}}\n`);
capture("terraform", ["fmt", ...TF_OUT]);
for (const out of TF_OUT) console.log(`Generated ${out}`);
