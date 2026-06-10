// Invoke a Lambda function remotely with its sample scheduled event payload.
// Usage: node scripts/trigger.mjs --function=<name> --stack=<stack> --profile=<aws-profile>
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { parseArgs } from "node:util";
import { awsEnv, capture, fail, requireOpts } from "./lib.mjs";

const USAGE = "node scripts/trigger.mjs --function=<name> --stack=<stack> --profile=<aws-profile>";

const { values } = parseArgs({
  options: {
    function: { type: "string", default: "" },
    stack: { type: "string", default: "" },
    profile: { type: "string", default: "" },
  },
});
requireOpts({ function: values.function, stack: values.stack, profile: values.profile }, USAGE);
const { function: fn, stack } = values;

const sample = `functions/${fn}/events/sample.json`;
if (!existsSync(sample)) fail(`Error: ${sample} not found`);

mkdirSync("payloads", { recursive: true });
const payloadFile = `payloads/${fn}.json`;
writeFileSync(payloadFile, readFileSync(sample, "utf8").replaceAll("my-stack", stack));

const responseFile = `payloads/${fn}.response.json`;
const invokeMetadata = capture(
  "aws",
  [
    "lambda",
    "invoke",
    "--function-name",
    `${stack}-${fn}`,
    "--payload",
    `fileb://${payloadFile}`,
    "--cli-binary-format",
    "raw-in-base64-out",
    responseFile,
  ],
  { env: awsEnv(values.profile) },
);
writeFileSync(`payloads/${fn}.invoke.json`, `${invokeMetadata}\n`);

const response = readFileSync(responseFile, "utf8");
if (response !== "null") {
  console.log("Function payload:");
  console.log(response);
  console.log("");
}

console.log("Invoke metadata:");
console.log(invokeMetadata);
