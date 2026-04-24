import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const __dirname = dirname(fileURLToPath(import.meta.url));
const DEFAULT_OUT_DIR = join(__dirname, "discovery");

function parseArgs() {
  const args = process.argv.slice(2);
  const outDirIndex = args.indexOf("--out-dir");
  if (outDirIndex === -1) {
    return { outDir: DEFAULT_OUT_DIR };
  }
  const outDir = args[outDirIndex + 1];
  if (!outDir) {
    throw new Error("--out-dir requires a path");
  }
  return { outDir: resolve(outDir) };
}

function readPromptfooVersion() {
  const packageJsonPath = join(__dirname, "node_modules", "promptfoo", "package.json");
  const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
  return packageJson.version;
}

function writeJson(path, payload) {
  writeFileSync(path, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
}

function extractAssertionResult(row) {
  const componentResults = row?.gradingResult?.componentResults;
  if (!Array.isArray(componentResults) || componentResults.length !== 1) {
    throw new Error("expected exactly one Promptfoo assertion component result");
  }
  const component = componentResults[0];
  const assertionType = component?.assertion?.type;
  if (assertionType !== "equals") {
    throw new Error(`expected equals assertion component, got ${assertionType}`);
  }
  return {
    pass: component.pass,
    score: component.score,
    reason: component.reason,
    assertion: component.assertion,
  };
}

function runPromptfoo(workDir, promptfooBin) {
  const result = spawnSync(
    promptfooBin,
    [
      "eval",
      "--assertions",
      "asserts.yaml",
      "--model-outputs",
      "outputs.json",
      "--output",
      "results.jsonl",
      "--no-table",
      "--no-write",
      "--no-progress-bar",
    ],
    {
      cwd: workDir,
      env: {
        ...process.env,
        PROMPTFOO_DISABLE_TELEMETRY: "1",
      },
      stdio: "inherit",
    },
  );
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0 && result.status !== 100) {
    throw new Error(`promptfoo exited with unexpected status ${result.status}`);
  }
}

function main() {
  const { outDir } = parseArgs();
  const promptfooBin = join(__dirname, "node_modules", ".bin", "promptfoo");
  const packageVersion = readPromptfooVersion();
  const workDir = mkdtempSync(join(tmpdir(), "assay-p28-promptfoo-"));

  const outputs = ["Hello world", "Goodbye world"];
  const assertions = [{ type: "equals", value: "Hello world" }];

  writeJson(join(workDir, "outputs.json"), outputs);
  writeFileSync(join(workDir, "asserts.yaml"), "- type: equals\n  value: Hello world\n", "utf8");

  runPromptfoo(workDir, promptfooBin);

  const rows = readFileSync(join(workDir, "results.jsonl"), "utf8")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));

  if (rows.length !== 2) {
    throw new Error(`expected two JSONL rows, got ${rows.length}`);
  }

  const discoveryInput = {
    sdk_language: "node",
    package: "promptfoo",
    package_version: packageVersion,
    surfaced_path: "cli-jsonl",
    model_outputs: outputs,
    assertions,
  };
  const validAssertion = extractAssertionResult(rows[0]);
  const failureAssertion = extractAssertionResult(rows[1]);

  writeJson(join(outDir, "promptfoo.inputs.json"), discoveryInput);
  writeJson(join(outDir, "valid.full-jsonl-row.json"), rows[0]);
  writeJson(join(outDir, "valid.surfaced.assertion-result.json"), validAssertion);
  writeJson(join(outDir, "failure.full-jsonl-row.json"), rows[1]);
  writeJson(join(outDir, "failure.surfaced.assertion-result.json"), failureAssertion);

  rmSync(workDir, { recursive: true, force: true });

  console.log(
    JSON.stringify(
      {
        package_version: packageVersion,
        surfaced_path: "cli-jsonl",
        valid_score: validAssertion.score,
        failure_score: failureAssertion.score,
      },
      null,
      2,
    ),
  );
}

main();
