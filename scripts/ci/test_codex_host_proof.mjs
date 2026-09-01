#!/usr/bin/env node
/**
 * Behavioral contracts for the Codex host-proof driver and validator.
 * Classification must be imported from the validator. Synthetic events are
 * never live proof. A successful tool event with a nonzero driver exit is
 * never a pass.
 */
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  parseArgs,
  resolveHostIdentity as resolveHostIdentityProduction,
  runProof,
} from "./codex_host_proof.mjs";
import {
  CELLS,
  DECIDE_INPUT,
  DECIDE_TOOL,
  EXPECTED_TOOLS,
  HARD_MAX_SNAPSHOT_BYTES,
  classifyRecord,
  consumeJourneyTopology,
  decidePrompt,
  driverOutcomeFrom,
  elicitationAcceptable,
  forbiddenProofRoot,
  projectClientRequestParams,
  sha256File,
  sha256Utf8,
  stableStringify,
  validateProofRoot,
  verifyLiveIdentity,
} from "./codex_host_proof_validator.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FAKE = path.join(HERE, "fixtures/codex-host-proof/fake-app-server.mjs");
const DRIVER_SRC = fs.readFileSync(path.join(HERE, "codex_host_proof.mjs"), "utf8");
const VALIDATOR_SRC = fs.readFileSync(path.join(HERE, "codex_host_proof_validator.mjs"), "utf8");

function scratch() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "assay-2684-"));
}

function resolveHostIdentity(options = {}) {
  return resolveHostIdentityProduction({
    proofRoot: options.proofRoot ?? scratch(),
    ...options,
  });
}

function spawnFakeChild(childArgv, projectRoot) {
  return spawn(childArgv[0], childArgv.slice(1), {
    stdio: ["pipe", "pipe", "pipe"],
    env: {
      PATH: process.env.PATH,
      HOME: projectRoot,
      CODEX_HOME: path.join(projectRoot, ".codex-home"),
    },
  });
}

async function drive(scenario, journey = "tool") {
  const projectRoot = scratch();
  const proofRoot = scratch();
  fs.mkdirSync(path.join(projectRoot, ".agents/skills/assay-golden-path"), {
    recursive: true,
  });
  fs.writeFileSync(
    path.join(projectRoot, ".agents/skills/assay-golden-path/SKILL.md"),
    "# disposable canary\n",
  );
  const childArgv = ["node", FAKE, "--scenario", scenario, "--project-root", projectRoot];
  const result = await runProof({
    captureMode: "synthetic-fixture",
    timeoutMs: 4000,
    maxBytes: 1_048_576,
    journey,
    allowLiveTurn: false,
    testOnlyChild: spawnFakeChild(childArgv, projectRoot),
    proofRoot,
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
  });
  return { ...result, proofRoot, projectRoot };
}

function seedProject() {
  const projectRoot = scratch();
  fs.mkdirSync(path.join(projectRoot, ".agents/skills/assay-golden-path"), {
    recursive: true,
  });
  fs.writeFileSync(
    path.join(projectRoot, ".agents/skills/assay-golden-path/SKILL.md"),
    "# disposable canary\n",
  );
  return projectRoot;
}

function writePortableNodeExecutable(filePath, esmSource) {
  const body = String(esmSource).replace(/^#![^\n]*\r?\n/, "");
  fs.writeFileSync(
    filePath,
    `#!/usr/bin/env node
void import(${JSON.stringify(`data:text/javascript,${encodeURIComponent(body)}`)});
`,
    { mode: 0o755 },
  );
  return filePath;
}

function writeShadowCodex(childArgv) {
  const binDir = scratch();
  const bin = path.join(binDir, "codex");
  writePortableNodeExecutable(
    bin,
    `import { spawn } from "node:child_process";
if (process.argv.includes("--version")) {
  process.stdout.write("codex-shadow/0.0.0\\n");
  process.exit(0);
}
const child = spawn(${JSON.stringify(childArgv[0])}, ${JSON.stringify(childArgv.slice(1))}, { stdio: "inherit" });
const stop = () => {
  try { child.kill("SIGTERM"); } catch { /* already exited */ }
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
child.on("close", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
`,
  );
  return bin;
}

function writeShadowMcp() {
  const bin = path.join(scratch(), "assay-mcp-server");
  writePortableNodeExecutable(
    bin,
    `if (process.argv.includes("--version")) {
  process.stdout.write("assay-mcp-server-shadow/0.0.0\\n");
  process.exit(0);
}
process.stdout.write("assay-mcp-server-shadow/0.0.0\\n");
`,
  );
  return bin;
}

function driveCli(scenario, journey = "tool", extra = {}) {
  const projectRoot = extra.projectRoot ?? seedProject();
  const proofRoot = extra.proofRoot ?? scratch();
  const mcpBin = extra.assayMcpBin ?? writeShadowMcp();
  const codexBin =
    extra.codexBin ??
    writeShadowCodex(["node", FAKE, "--scenario", scenario, "--project-root", projectRoot]);
  const args = [
    path.join(HERE, "codex_host_proof.mjs"),
    "--capture-mode",
    extra.captureMode ?? "synthetic-fixture",
    "--proof-root",
    proofRoot,
    "--project-root",
    projectRoot,
    "--journey",
    journey,
    "--timeout-ms",
    String(extra.timeoutMs ?? 4000),
  ];
  if (extra.allowLiveTurn) {
    args.push("--allow-live-turn");
  }
  if (extra.extraArgs) {
    args.push(...extra.extraArgs);
  }
  const result = spawnSync(process.execPath, args, {
    encoding: "utf8",
    timeout: 15_000,
    env: {
      ...process.env,
      PATH: `${path.dirname(codexBin)}${path.delimiter}${path.dirname(mcpBin)}${path.delimiter}${process.env.PATH}`,
    },
  });
  return { ...result, proofRoot, projectRoot, mcpBin, codexBin };
}

function clientParams(events, method) {
  return events.filter(
    (event) => event.direction === "client" && event.method === method,
  );
}

test("driver calls the validator classification function; no extra classify module", () => {
  assert.match(DRIVER_SRC, /from "\.\/codex_host_proof_validator\.mjs"/);
  assert.doesNotMatch(DRIVER_SRC, /codex_host_proof_classify/);
  assert.doesNotMatch(VALIDATOR_SRC, /codex_host_proof_classify/);
  assert.equal(fs.existsSync(path.join(HERE, "codex_host_proof_classify.mjs")), false);
});

test("synthetic positive control: cells pass without inventing external attestation", async () => {
  const { classified, manifest, proofRoot, driverOutcome, childExitCode, events } = await drive("valid");
  assert.equal(manifest.captureMode, "synthetic-fixture");
  assert.equal(manifest.schema, "assay.codex-host-proof.v3");
  assert.equal(childExitCode, 0);
  assert.equal(driverOutcome.exitCode, 0);
  assert.equal(manifest.childExitCode, 0);
  assert.equal(manifest.driverOutcome.exitCode, 0);
  for (const name of CELLS) {
    assert.equal(
      classified.cells[name].status,
      "pass",
      `${name} must pass on the nominal synthetic pack`,
    );
  }
  assert.equal(classified.externalAttestation, "not_provided");
  for (const event of clientParams(events, "mcpServerStatus/list")) {
    assert.equal(typeof event.params.threadId, "string");
    assert.notEqual(event.params.threadId, "");
  }
  for (const event of clientParams(events, "turn/start")) {
    assert.equal(typeof event.params.threadId, "string");
    assert.notEqual(event.params.threadId, "");
  }
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, true);
  assert.equal(checked.classified.externalAttestation, "not_provided");
});

test("no-op discovery control does not invent tool or MCP passes", async () => {
  const { classified } = await drive("valid", "discovery");
  assert.notEqual(classified.cells.oneToolInvoked.status, "pass");
  assert.notEqual(classified.cells.mcpStarted.status, "pass");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("successful tool output plus nonzero child exit: CLI process is nonzero", async () => {
  const { classified, manifest, proofRoot, childExitCode, driverOutcome } = await drive(
    "exit-1-after-success",
  );
  assert.equal(childExitCode, 1);
  assert.notEqual(driverOutcome.exitCode, 0);
  assert.equal(manifest.childExitCode, 1);
  assert.notEqual(manifest.driverOutcome.exitCode, 0);
  assert.equal(classified.cells.oneToolInvoked.status, "pass");
  assert.notEqual(classified.externalAttestation, "pass");
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, true);
  assert.notEqual(checked.classified.externalAttestation, "pass");
  const relabeled = classifyRecord({
    ...manifest,
    events: JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8")),
    childExitCode: 0,
    driverOutcome: { exitCode: 0, status: "pass" },
    captureMode: "host-observation",
  });
  assert.notEqual(
    relabeled.externalAttestation,
    "pass",
    "rewriting exit 0 on synthetic fake events must not mint live proof",
  );
  const cli = driveCli("exit-1-after-success");
  assert.notEqual(cli.status, 0, "driver CLI must not exit 0 when the child exits 1");
  assert.match(cli.stdout, /"exitCode":1/);
});

test("synthetic events never validate as actual-host proof", async () => {
  const { manifest, proofRoot, classified } = await drive("valid");
  assert.notEqual(classified.externalAttestation, "pass");
  const forged = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
  forged.captureMode = "host-observation";
  fs.writeFileSync(
    path.join(proofRoot, "manifest.json"),
    `${JSON.stringify(forged)}\n`,
  );
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, false);
  assert.match(checked.reasons.join(" "), /host observation|fake|synthetic/i);
});

test("missing skill is not pass", async () => {
  const { classified } = await drive("missing-skill");
  assert.notEqual(classified.cells.skillDiscovered.status, "pass");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("wrong cwd is not pass", async () => {
  const { classified } = await drive("wrong-cwd");
  assert.notEqual(classified.cells.cwdObserved.status, "pass");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("missing tool is not a clean tool list", async () => {
  const { classified } = await drive("missing-tool");
  assert.notEqual(classified.cells.exactToolsListed.status, "pass");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("wrong tool invocation is not pass", async () => {
  const { classified } = await drive("wrong-tool");
  assert.equal(classified.cells.oneToolInvoked.status, "fail");
  assert.notEqual(classified.cells.oneToolInvoked.status, "unavailable");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("clean missing-binary status is not a host-failure pass", async () => {
  const { classified } = await drive("clean-missing-binary");
  assert.notEqual(classified.cells.missingBinaryNotClean.status, "pass");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("clean invalid-policy-root status is not a host-failure pass", async () => {
  const { classified } = await drive("clean-invalid-root");
  assert.notEqual(classified.cells.invalidPolicyRootNotClean.status, "pass");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("truncated stream does not pass", async () => {
  const { classified } = await drive("truncated");
  assert.notEqual(classified.externalAttestation, "pass");
  assert.notEqual(classified.cells.driverCompleted.status, "pass");
});

test("unavailable stream does not pass", async () => {
  const { classified } = await drive("unavailable-stream");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("hash mismatch fails the validator", async () => {
  const { proofRoot } = await drive("valid");
  const eventsPath = path.join(proofRoot, "events.json");
  const events = JSON.parse(fs.readFileSync(eventsPath, "utf8"));
  events.push({ direction: "tamper", method: "none" });
  fs.writeFileSync(eventsPath, `${JSON.stringify(events)}\n`);
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, false);
  assert.match(checked.reasons.join(" "), /hash/);
});

test("proof membership is exact: extra file, directory, or symlink fail; control stays ok", async () => {
  const { proofRoot } = await drive("valid");
  assert.equal(validateProofRoot(proofRoot).ok, true);
  const extra = path.join(proofRoot, "unexpected-canary.txt");
  fs.writeFileSync(extra, "NONSECRET_EXTRA_FILE\n");
  const withFile = validateProofRoot(proofRoot);
  assert.equal(withFile.ok, false);
  assert.match(withFile.reasons.join(" "), /membership|allowlist|extra/i);
  fs.unlinkSync(extra);
  assert.equal(validateProofRoot(proofRoot).ok, true);
  const extraDir = path.join(proofRoot, "extra-dir");
  fs.mkdirSync(extraDir);
  const withDir = validateProofRoot(proofRoot);
  assert.equal(withDir.ok, false);
  fs.rmdirSync(extraDir);
  assert.equal(validateProofRoot(proofRoot).ok, true);
  const link = path.join(proofRoot, "events.json.link");
  fs.symlinkSync(path.join(proofRoot, "events.json"), link);
  const withLink = validateProofRoot(proofRoot);
  assert.equal(withLink.ok, false);
  fs.unlinkSync(link);
  assert.equal(validateProofRoot(proofRoot).ok, true);
});

test("fake app-server rejects missing and unknown thread IDs", async () => {
  const projectRoot = seedProject();
  const child = spawn(
    process.execPath,
    [FAKE, "--scenario", "valid", "--project-root", projectRoot],
    { stdio: ["pipe", "pipe", "pipe"] },
  );
  const chunks = [];
  child.stdout.on("data", (chunk) => {
    chunks.push(chunk);
  });
  const send = (message) => {
    child.stdin.write(`${JSON.stringify(message)}\n`);
  };
  send({
    id: 1,
    method: "initialize",
    params: { clientInfo: { name: "t", version: "1" } },
  });
  send({
    id: 2,
    method: "thread/start",
    params: {
      cwd: projectRoot,
      config: { mcp_servers: { assay: { command: "x", args: ["--policy-root", "."] } } },
    },
  });
  send({ id: 3, method: "mcpServerStatus/list", params: { threadId: null } });
  send({ id: 4, method: "mcpServerStatus/list", params: { threadId: "unknown-thread" } });
  send({
    id: 5,
    method: "turn/start",
    params: { threadId: null, input: [{ type: "text", text: "x" }] },
  });
  child.stdin.end();
  const status = await new Promise((resolve) => {
    child.on("close", (code) => resolve(code));
  });
  assert.equal(status, 0);
  const replies = Buffer.concat(chunks)
    .toString("utf8")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  const byId = Object.fromEntries(replies.filter((row) => row.id != null).map((row) => [row.id, row]));
  assert.ok(byId[3]?.error, "null threadId must be an error, not a primary fallback");
  assert.ok(byId[4]?.error, "unknown threadId must be an error, not a primary fallback");
  assert.ok(byId[5]?.error, "turn/start without a known threadId must error");
  assert.equal(replies.some((row) => row.method === "item/completed"), false);
});

function proofFiles(proofRoot) {
  return fs.existsSync(proofRoot) ? fs.readdirSync(proofRoot).sort() : [];
}

function toolCompleted(events) {
  return events.find(
    (event) =>
      event.method === "item/completed" &&
      event.direction === "server" &&
      event.params?.item?.type === "mcpToolCall",
  );
}

function syncTerminalToolItem(events) {
  const item = toolCompleted(events)?.params?.item;
  const terminal = events.find(
    (event) => event.direction === "server" && event.method === "turn/completed",
  );
  assert.ok(item);
  assert.ok(terminal?.params?.turn);
  terminal.params.turn.items = [structuredClone(item)];
}

function driveInline(childArgv, extra = {}) {
  const projectRoot = extra.projectRoot ?? seedProject();
  const proofRoot = extra.proofRoot ?? scratch();
  const testOnlyChild = extra.testOnlyChild ?? spawnFakeChild(childArgv, projectRoot);
  return runProof({
    captureMode: extra.captureMode ?? "synthetic-fixture",
    timeoutMs: extra.timeoutMs ?? 4000,
    maxBytes: extra.maxBytes ?? 1_048_576,
    journey: extra.journey ?? "tool",
    allowLiveTurn: extra.allowLiveTurn ?? false,
    testOnlyChild,
    proofRoot,
    projectRoot,
    assayMcpBin: extra.assayMcpBin ?? path.join(projectRoot, "install/bin/assay-mcp-server"),
    hostIdentity: extra.hostIdentity,
  }).then((result) => ({ ...result, proofRoot, projectRoot }));
}

function driveCliInline(childArgv, extra = {}) {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const mcpBin = extra.assayMcpBin ?? writeShadowMcp();
  const codexBin = writeShadowCodex(childArgv);
  const args = [
    path.join(HERE, "codex_host_proof.mjs"),
    "--capture-mode",
    "synthetic-fixture",
    "--proof-root",
    proofRoot,
    "--project-root",
    projectRoot,
    "--journey",
    extra.journey ?? "tool",
    "--timeout-ms",
    String(extra.timeoutMs ?? 4000),
  ];
  if (extra.maxBytes != null) {
    args.push("--max-bytes", String(extra.maxBytes));
  }
  const result = spawnSync(process.execPath, args, {
    encoding: "utf8",
    timeout: 15_000,
    env: {
      ...process.env,
      PATH: `${path.dirname(codexBin)}${path.delimiter}${path.dirname(mcpBin)}${path.delimiter}${process.env.PATH}`,
    },
  });
  return { ...result, proofRoot, projectRoot };
}

test("finite oversize line is bounded before parse; cells stay unavailable", async () => {
  const oversize = 8192;
  const childArgv = [
    process.execPath,
    "-e",
    `process.stdout.write(JSON.stringify({method:"probe/finite",params:{text:"x".repeat(${oversize})}})+"\\n");process.stdin.resume();`,
  ];
  const { classified, manifest, proofRoot, events } = await driveInline(childArgv, {
    maxBytes: 1024,
    timeoutMs: 800,
  });
  const eventsBytes = fs.statSync(path.join(proofRoot, "events.json")).size;
  assert.equal(manifest.truncated, true);
  assert.ok(eventsBytes < 4096, `events.json ${eventsBytes} retained the oversize line`);
  assert.equal(
    JSON.stringify(events).includes("x".repeat(256)),
    false,
    "parser must not retain the oversize payload",
  );
  assert.notEqual(classified.cells.oneToolInvoked.status, "pass");
  assert.equal(classified.cells.oneToolInvoked.status, "unavailable");
  assert.notEqual(classified.externalAttestation, "pass");
  const checked = validateProofRoot(proofRoot);
  assert.notEqual(checked.classified.externalAttestation, "pass");
});

test("malformed JSON line writes unavailable evidence; CLI proof root is not empty", async () => {
  const childArgv = [
    process.execPath,
    "-e",
    "process.stdout.write('{not json}\\n');process.stdin.resume();",
  ];
  const cli = driveCliInline(childArgv, { timeoutMs: 800 });
  assert.equal(cli.stderr.includes("SyntaxError"), false, "parse errors must not escape the data callback");
  assert.deepEqual(proofFiles(cli.proofRoot), [
    "assay-mcp-server.snapshot",
    "classification.json",
    "codex.snapshot",
    "events.json",
    "manifest.json",
  ]);
  const stored = JSON.parse(
    fs.readFileSync(path.join(cli.proofRoot, "classification.json"), "utf8"),
  );
  assert.equal(stored.cells.oneToolInvoked.status, "unavailable");
  assert.notEqual(stored.cells.oneToolInvoked.status, "pass");
  assert.notEqual(stored.externalAttestation, "pass");
  const events = JSON.parse(fs.readFileSync(path.join(cli.proofRoot, "events.json"), "utf8"));
  const driverErrors = events.filter(
    (event) => event.direction === "driver" && event.method === "error",
  );
  assert.ok(driverErrors.length >= 1, "the failed run must retain a driver error witness");
  assert.deepEqual(
    [...new Set(driverErrors.map((event) => event.params?.message))],
    ["retained driver error"],
    "driver failures must be the same closed projection the validator recomputes",
  );
});

test("production driver creates and verifies its disposable CODEX_HOME before spawn", () => {
  const projectRoot = seedProject();
  const codexHome = path.join(projectRoot, ".codex-home");
  const retainedFilesWithIdentity = [
    "assay-mcp-server.snapshot",
    "classification.json",
    "codex.snapshot",
    "events.json",
    "manifest.json",
  ];
  assert.equal(fs.existsSync(codexHome), false, "control starts without CODEX_HOME");

  const binDir = scratch();
  const codexBin = path.join(binDir, "codex");
  writePortableNodeExecutable(
    codexBin,
    `import fs from "node:fs";
import { spawn } from "node:child_process";
if (process.argv.includes("--version")) {
  process.stdout.write("codex-home-check/1.0.0\\n");
  process.exit(0);
}
fs.writeFileSync(process.env.HOME + "/.codex-app-server-started", "1");
const stat = fs.statSync(process.env.CODEX_HOME);
if (!stat.isDirectory() || (process.platform !== "win32" && (stat.mode & 0o7777) !== 0o700)) {
  process.exit(73);
}
const child = spawn(${JSON.stringify(process.execPath)}, ${JSON.stringify([FAKE, "--scenario", "valid", "--project-root", projectRoot])}, { stdio: "inherit" });
const stop = () => { try { child.kill("SIGTERM"); } catch { /* already exited */ } };
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
child.on("close", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
`,
  );

  const cli = driveCli("valid", "discovery", { projectRoot, codexBin });
  assert.equal(cli.status, 0, cli.stderr || cli.stdout);
  const stat = fs.lstatSync(codexHome);
  assert.equal(stat.isSymbolicLink(), false);
  assert.equal(stat.isDirectory(), true);
  if (process.platform !== "win32") {
    assert.equal(stat.mode & 0o7777, 0o700);
  }

  const fileProjectRoot = seedProject();
  fs.writeFileSync(path.join(fileProjectRoot, ".codex-home"), "not a directory\n");
  const fileCli = driveCli("valid", "discovery", {
    projectRoot: fileProjectRoot,
    codexBin,
  });
  assert.notEqual(fileCli.status, 0);
  assert.deepEqual(
    proofFiles(fileCli.proofRoot),
    retainedFilesWithIdentity,
    "a rejected CODEX_HOME must still leave a complete retained failure record",
  );
  const fileEvents = JSON.parse(
    fs.readFileSync(path.join(fileCli.proofRoot, "events.json"), "utf8"),
  );
  assert.deepEqual(fileEvents, [
    {
      direction: "driver",
      method: "error",
      params: { message: "retained driver error" },
    },
  ]);
  const fileManifest = JSON.parse(
    fs.readFileSync(path.join(fileCli.proofRoot, "manifest.json"), "utf8"),
  );
  assert.notEqual(fileManifest.driverOutcome.exitCode, 0);
  assert.equal(validateProofRoot(fileCli.proofRoot).ok, true);
  assert.equal(
    fs.existsSync(path.join(fileProjectRoot, ".codex-app-server-started")),
    false,
    "a non-directory CODEX_HOME must be rejected before app-server spawn",
  );

  const hostProjectRoot = seedProject();
  fs.writeFileSync(path.join(hostProjectRoot, ".codex-home"), "not a directory\n");
  const hostProofRoot = portableLiveProofRoot();
  try {
    const hostCli = driveCli("valid", "discovery", {
      captureMode: "host-observation",
      projectRoot: hostProjectRoot,
      proofRoot: hostProofRoot,
      codexBin,
    });
    assert.notEqual(hostCli.status, 0);
    assert.deepEqual(
      proofFiles(hostCli.proofRoot),
      retainedFilesWithIdentity,
      `host-observation failure must retain its proof-owned subjects: ${hostCli.stderr || hostCli.stdout}`,
    );
    assert.equal(validateProofRoot(hostCli.proofRoot).ok, true);
    assert.equal(
      fs.existsSync(path.join(hostProjectRoot, ".codex-app-server-started")),
      false,
      "host-observation must reject unsafe CODEX_HOME before app-server spawn",
    );
  } finally {
    fs.rmSync(hostProofRoot, { recursive: true, force: true });
  }

  if (process.platform !== "win32") {
    const publicProjectRoot = seedProject();
    const publicCodexHome = path.join(publicProjectRoot, ".codex-home");
    fs.mkdirSync(publicCodexHome, { mode: 0o755 });
    fs.chmodSync(publicCodexHome, 0o755);
    const publicCli = driveCli("valid", "discovery", {
      projectRoot: publicProjectRoot,
      codexBin,
    });
    assert.notEqual(publicCli.status, 0);
    assert.equal(
      fs.existsSync(path.join(publicProjectRoot, ".codex-app-server-started")),
      false,
      "a non-private CODEX_HOME must be rejected before app-server spawn",
    );
  }
});

test("credential-bearing argv is rejected before spawn and not persisted", async () => {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const marker = "NONSECRET_PROBE_MARKER";
  const { classified, manifest, events } = await runProof({
    captureMode: "synthetic-fixture",
    timeoutMs: 4000,
    maxBytes: 1_048_576,
    journey: "tool",
    allowLiveTurn: false,
    childArgv: [
      "node",
      FAKE,
      "--scenario",
      "valid",
      "--project-root",
      projectRoot,
      `--api-key=${marker}`,
    ],
    proofRoot,
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
  });
  const packed = [
    JSON.stringify(manifest),
    JSON.stringify(events),
    JSON.stringify(classified),
  ].join("\n");
  assert.equal(packed.includes(marker), false);
  assert.equal(
    events.some((event) => event.method === "initialize"),
    false,
    "credential argv must be rejected before spawn",
  );
  assert.equal(classified.cells.oneToolInvoked.status, "unavailable");
  assert.notEqual(classified.externalAttestation, "pass");
  const control = await drive("valid");
  assert.equal(
    JSON.stringify(control.manifest.invocation.argv).includes(marker),
    false,
  );
  assert.equal(control.classified.cells.skillDiscovered.status, "pass");
});

test("wrong server, thread, or turn cannot pass invocation or result cells", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(classified.cells.oneToolInvoked.status, "pass");
  assert.equal(classified.cells.structuredResultValidated.status, "pass");
  const base = { ...manifest, events };
  const mutate = (fn) => {
    const copy = structuredClone(base);
    fn(copy.events);
    return classifyRecord(copy);
  };
  const tool = () => {
    const event = toolCompleted(base.events);
    assert.ok(event);
    return event;
  };
  const wrongServer = mutate((ev) => {
    toolCompleted(ev).params.item.server = "not-assay";
  });
  const wrongThread = mutate((ev) => {
    toolCompleted(ev).params.threadId = "unrelated-thread";
  });
  const wrongTurn = mutate((ev) => {
    toolCompleted(ev).params.turnId = "unrelated-turn";
  });
  for (const [label, result] of [
    ["wrong-server", wrongServer],
    ["wrong-thread", wrongThread],
    ["wrong-turn", wrongTurn],
  ]) {
    assert.notEqual(
      result.cells.oneToolInvoked.status,
      "pass",
      `${label} must not pass oneToolInvoked`,
    );
    assert.notEqual(
      result.cells.structuredResultValidated.status,
      "pass",
      `${label} must not pass structuredResultValidated`,
    );
  }
  assert.equal(tool().params.item.server, "assay");
});

test("wait for expected tool, not an earlier userMessage; delayed control still passes", async () => {
  const delayed = await drive("delayed-tool");
  assert.equal(delayed.classified.cells.oneToolInvoked.status, "pass");
  assert.notEqual(delayed.classified.externalAttestation, "pass");
  const interleaved = await drive("early-user-then-tool");
  assert.equal(
    interleaved.classified.cells.oneToolInvoked.status,
    "pass",
    "early userMessage must not close the wait before the delayed tool",
  );
  const items = interleaved.events
    .filter((event) => event.method === "item/completed")
    .map((event) => event.params?.item?.type);
  assert.deepEqual(items, ["userMessage", "mcpToolCall"]);
  const failedAt = Date.now();
  const failed = await drive("turn-failed");
  assert.ok(
    Date.now() - failedAt < 1500,
    "terminal turn failure must not wait out the full timeout",
  );
  assert.equal(failed.classified.cells.oneToolInvoked.status, "unavailable");
  assert.notEqual(failed.classified.cells.oneToolInvoked.status, "pass");
  assert.notEqual(failed.classified.externalAttestation, "pass");
});

test("turn/start prompt is derived from DECIDE_TOOL/DECIDE_INPUT; response uses result.turn.id", async () => {
  const { events } = await drive("valid");
  const sent = clientParams(events, "turn/start")[0];
  const text = sent.params.input[0].text;
  assert.equal(text, decidePrompt());
  assert.match(text, new RegExp(DECIDE_TOOL.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  assert.match(text, new RegExp(DECIDE_INPUT.tool.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  assert.match(text, new RegExp(DECIDE_INPUT.policy.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  const reply = events.find(
    (event) => event.method === "turn/start" && event.direction === "server",
  );
  assert.equal(typeof reply.result.turn.id, "string");
  assert.equal(reply.result.turnId, undefined);
  assert.doesNotMatch(DRIVER_SRC, /result\?\.turnId|result\.turnId/);
  assert.doesNotMatch(DRIVER_SRC, /\.isError\b/);
});

test("validator reads allowlisted files with a bounded read, not a full slurp then check", async () => {
  const { proofRoot } = await drive("valid");
  assert.equal(validateProofRoot(proofRoot).ok, true);
  const huge = path.join(proofRoot, "events.json");
  fs.writeFileSync(huge, `${"["}${"0,".repeat(2000)}0]`);
  const checked = validateProofRoot(proofRoot, 1024);
  assert.equal(checked.ok, false);
  assert.match(checked.reasons.join(" "), /exceeds byte bound|unavailable/);
  assert.match(VALIDATOR_SRC, /readSync/);
  assert.doesNotMatch(
    VALIDATOR_SRC,
    /readFileSync\(path\.join\(proofRoot/,
  );
});

test("maxBytes and timeoutMs must be finite positive integers; reversal restores a working cap", async () => {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const base = {
    captureMode: "synthetic-fixture",
    journey: "tool",
    allowLiveTurn: false,
    childArgv: ["node", FAKE, "--scenario", "valid", "--project-root", projectRoot],
    proofRoot,
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
    timeoutMs: 4000,
    maxBytes: 1_048_576,
  };
  await assert.rejects(() => runProof({ ...base, maxBytes: 0 }), /finite positive/);
  await assert.rejects(() => runProof({ ...base, timeoutMs: Number.POSITIVE_INFINITY }), /finite positive/);
  const control = await drive("valid");
  assert.equal(control.classified.cells.driverCompleted.status, "pass");
});

test("forged live captureMode and initialize cannot mint a live pass from synthetic events", async () => {
  const { manifest, proofRoot, events, classified } = await drive("valid");
  assert.notEqual(classified.externalAttestation, "pass");
  const forged = {
    ...JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8")),
    captureMode: "host-observation",
    initialize: {
      codexHome: "/opt/codex-home",
      userAgent: "codex_cli/0.50.0",
      platformFamily: "unix",
      platformOs: "linux",
    },
  };
  const relabeled = classifyRecord({
    ...forged,
    events,
    childExitCode: 0,
    driverOutcome: { exitCode: 0, status: "pass" },
    truncated: false,
    streamUnavailable: false,
  });
  assert.notEqual(
    relabeled.externalAttestation,
    "pass",
    "mutable initialize/userAgent must not relabel synthetic events as live",
  );
  fs.writeFileSync(path.join(proofRoot, "manifest.json"), stableStringify(forged));
  fs.writeFileSync(
    path.join(proofRoot, "classification.json"),
    stableStringify(relabeled),
  );
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, false);
  assert.match(
    checked.reasons.join(" "),
    /initialize|parity|identity|fake|synthetic|userAgent|binary/i,
  );
  assert.notEqual(checked.classified?.externalAttestation, "pass");
});

test("production CLI rejects --child-argv; credential name variants are rejected before spawn", async () => {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const cli = spawnSync(
    process.execPath,
    [
      path.join(HERE, "codex_host_proof.mjs"),
      "--capture-mode",
      "synthetic-fixture",
      "--proof-root",
      proofRoot,
      "--project-root",
      projectRoot,
      "--child-argv",
      JSON.stringify(["node", FAKE, "--scenario", "valid", "--project-root", projectRoot]),
    ],
    { encoding: "utf8", timeout: 10_000 },
  );
  assert.notEqual(cli.status, 0, "production CLI must not accept free-form --child-argv");
  assert.match(`${cli.stderr}${cli.stdout}`, /child-argv|unknown argument/i);
  const marker = "NONSECRET_UNDERSCORE_KEY";
  for (const flag of [`--api_key=${marker}`, `--API_KEY=${marker}`, `--api-key=${marker}`]) {
    const result = await runProof({
      captureMode: "synthetic-fixture",
      timeoutMs: 4000,
      maxBytes: 1_048_576,
      journey: "tool",
      allowLiveTurn: false,
      childArgv: ["node", FAKE, "--scenario", "valid", "--project-root", projectRoot, flag],
      proofRoot: scratch(),
      projectRoot,
      assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
    });
    assert.equal(
      result.events.some((event) => event.method === "initialize"),
      false,
      `${flag} must be rejected before spawn`,
    );
    assert.equal(
      JSON.stringify(result.manifest).includes(marker),
      false,
      `${flag} must not persist the credential value`,
    );
  }
});

test("hard maxima and one shared deadline bound frames, bytes, events, and waits", async () => {
  const projectRoot = seedProject();
  const base = {
    captureMode: "synthetic-fixture",
    journey: "discovery",
    allowLiveTurn: false,
    childArgv: ["node", FAKE, "--scenario", "valid", "--project-root", projectRoot],
    proofRoot: scratch(),
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
    timeoutMs: 4000,
    maxBytes: 1_048_576,
  };
  await assert.rejects(
    () => runProof({ ...base, proofRoot: scratch(), maxBytes: 50 * 1024 * 1024 }),
    /hard maximum|exceeds/,
  );
  const flood = await driveInline(
    [
      process.execPath,
      "-e",
      `for (let i = 0; i < 5000; i += 1) process.stdout.write(JSON.stringify({method:"flood",params:{i}})+"\\n");process.stdin.resume();`,
    ],
    { maxBytes: 1_048_576, timeoutMs: 2000, journey: "discovery" },
  );
  assert.ok(
    flood.manifest.truncated ||
      flood.manifest.streamUnavailable ||
      flood.events.length <= 4096,
    `cumulative events ${flood.events.length} must hit a hard operation cap`,
  );
  assert.notEqual(flood.classified.externalAttestation, "pass");
  const slow = await driveInline(
    [
      process.execPath,
      "-e",
      `let buf="";const reply=(o)=>setTimeout(()=>process.stdout.write(JSON.stringify(o)+"\\n"),150);process.stdin.on("data",(c)=>{buf+=c;let i;while((i=buf.indexOf("\\n"))>=0){const line=buf.slice(0,i);buf=buf.slice(i+1);if(!line.trim())continue;const m=JSON.parse(line);if(m.method==="initialize")reply({id:m.id,result:{userAgent:"assay-codex-host-proof-fake/1",codexHome:"/tmp/x"}});else if(m.method==="skills/list")reply({id:m.id,result:{data:[]}});}});process.stdin.resume();`,
    ],
    { timeoutMs: 200, journey: "discovery" },
  );
  assert.ok(
    slow.manifest.streamUnavailable ||
      slow.events.some((event) => /timeout/i.test(String(event.params?.message ?? ""))),
    "one absolute deadline must expire across wait phases; phases must not reset it",
  );
});

test("proof root rejects CODEX_HOME equality and outside-root events.json symlink", async () => {
  const { proofRoot } = await drive("valid");
  assert.equal(validateProofRoot(proofRoot).ok, true);
  const previousHome = process.env.CODEX_HOME;
  process.env.CODEX_HOME = proofRoot;
  try {
    const reason = forbiddenProofRoot(proofRoot, "synthetic-fixture");
    assert.equal(typeof reason, "string");
    assert.match(reason, /CODEX_HOME|runtime|profile|auth/i);
  } finally {
    if (previousHome === undefined) {
      delete process.env.CODEX_HOME;
    } else {
      process.env.CODEX_HOME = previousHome;
    }
  }
  const outside = path.join(os.tmpdir(), `assay-2684-outside-${process.pid}.json`);
  fs.renameSync(path.join(proofRoot, "events.json"), outside);
  fs.symlinkSync(outside, path.join(proofRoot, "events.json"));
  const linked = validateProofRoot(proofRoot);
  assert.equal(linked.ok, false, "outside-root events.json symlink must fail");
  assert.match(linked.reasons.join(" "), /symlink|regular file|nofollow|ELOOP|allowlist|unavailable/i);
  try {
    fs.unlinkSync(path.join(proofRoot, "events.json"));
  } catch {
    /* already removed */
  }
  try {
    fs.unlinkSync(outside);
  } catch {
    /* leftover */
  }
});

test("driver exits nonzero for truncated or unavailable streams even when the child exits 0", async () => {
  const unavailable = await drive("unavailable-stream");
  assert.equal(unavailable.childExitCode, 0);
  assert.notEqual(
    unavailable.driverOutcome.exitCode,
    0,
    "unavailable stream must not inherit a clean child exit",
  );
  assert.equal(unavailable.manifest.childExitCode, 0);
  assert.notEqual(unavailable.manifest.driverOutcome.exitCode, 0);
  const parseExit = await driveInline(
    [
      process.execPath,
      "-e",
      "process.stdout.write('{not json}\\n');process.exit(0);",
    ],
    { timeoutMs: 800, journey: "discovery" },
  );
  assert.notEqual(parseExit.driverOutcome.exitCode, 0);
  assert.notEqual(parseExit.manifest.driverOutcome.exitCode, 0);
  const failedValidation = await drive("missing-skill");
  assert.equal(failedValidation.childExitCode, 0);
  assert.notEqual(
    failedValidation.driverOutcome.exitCode,
    0,
    "failed validation must not inherit a clean child exit",
  );
  const cli = driveCliInline(
    [process.execPath, "-e", "process.stdout.write('{not json}\\n');process.exit(0);"],
    { timeoutMs: 800, journey: "discovery" },
  );
  assert.notEqual(cli.status, 0, "CLI must exit nonzero when the proof journey failed");
});

test("live mode declines elicitation that is not from the assay server", async () => {
  const { events } = await drive("foreign-elicit");
  const replies = events.filter(
    (event) =>
      event.direction === "client" && event.method === "mcpServer/elicitation/request",
  );
  assert.ok(replies.length >= 1, "foreign elicitation must be recorded");
  assert.equal(replies[0].result.action, "decline");
  const control = await drive("valid");
  const accepted = control.events.filter(
    (event) =>
      event.direction === "client" && event.method === "mcpServer/elicitation/request",
  );
  assert.equal(accepted[0].result.action, "accept");
});

test("skill path containment rejects a project-root-sibling prefix", async () => {
  const { events, manifest } = await drive("valid");
  const sibling = `${manifest.expected.projectRoot}-sibling`;
  const mutated = structuredClone({ ...manifest, events });
  for (const event of mutated.events) {
    if (event.method !== "skills/list" || event.direction !== "server") {
      continue;
    }
    for (const entry of event.result?.data ?? []) {
      for (const skill of entry.skills ?? []) {
        skill.path = path.join(sibling, ".agents/skills/assay-golden-path/SKILL.md");
      }
    }
  }
  const result = classifyRecord(mutated);
  assert.notEqual(
    result.cells.skillDiscovered.status,
    "pass",
    "string-prefix sibling of projectRoot must not count as contained",
  );
});

test("CLI entrypoint compares canonical realpaths so aliased tmp is not a no-op", () => {
  const realDir = fs.mkdtempSync(path.join(os.tmpdir(), "assay-2684-cli-real-"));
  fs.copyFileSync(path.join(HERE, "codex_host_proof.mjs"), path.join(realDir, "codex_host_proof.mjs"));
  fs.copyFileSync(
    path.join(HERE, "codex_host_proof_validator.mjs"),
    path.join(realDir, "codex_host_proof_validator.mjs"),
  );
  const linkDir = `${realDir}-link`;
  fs.symlinkSync(realDir, linkDir);
  const aliased = path.join(linkDir, "codex_host_proof.mjs");
  const result = spawnSync(process.execPath, [aliased], {
    encoding: "utf8",
    timeout: 10_000,
  });
  assert.notEqual(result.status, 0, "aliased CLI path must still enter main()");
  assert.match(result.stderr, /proof-root|project-root/i);
});

function portableLiveProofRoot() {
  const nest = path.join(os.userInfo().homedir, ".cache", "assay-ci-codex-host-proof");
  fs.mkdirSync(nest, { recursive: true });
  const root = fs.mkdtempSync(path.join(nest, `proof-${process.pid}-`));
  const reason = forbiddenProofRoot(root, "host-observation");
  if (reason) {
    fs.rmSync(root, { recursive: true, force: true });
    throw new Error(`test helper allocated a forbidden live proof root: ${reason}`);
  }
  return root;
}

test("production host identity is observed from proof-owned binaries before CLI exit 0", () => {
  assert.equal(
    forbiddenProofRoot(path.join("/tmp", `assay-live-reject-${process.pid}`), "host-observation"),
    "host-observation root must not be temporary storage",
  );
  const proofRoot = portableLiveProofRoot();
  try {
    const mcpBin = writeShadowMcp();
    const live = driveCli("valid", "tool", {
      captureMode: "host-observation",
      allowLiveTurn: true,
      assayMcpBin: mcpBin,
      proofRoot,
    });
    const manifestPath = path.join(live.proofRoot, "manifest.json");
    assert.equal(fs.existsSync(manifestPath), true, "live CLI must still write a pack");
    const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
    const identity = manifest.hostIdentity;
    assert.ok(identity && typeof identity === "object", "production CLI must construct hostIdentity");
    assert.deepEqual(Object.keys(identity).sort(), ["assayMcp", "codex"]);
    for (const role of ["codex", "assayMcp"]) {
      assert.deepEqual(Object.keys(identity[role]).sort(), ["path", "sha256"]);
      assert.equal(path.isAbsolute(identity[role].path), true, `${role} path must be absolute`);
      assert.match(identity[role].sha256, /^[a-f0-9]{64}$/);
      assert.equal(fs.lstatSync(identity[role].path).isFile(), true);
    }
    assert.equal(identity.assayMcp.sha256, sha256File(mcpBin));
    assert.equal(sha256File(identity.assayMcp.path), sha256File(mcpBin));
    const events = JSON.parse(fs.readFileSync(path.join(live.proofRoot, "events.json"), "utf8"));
    const start = events.find(
      (event) => event.direction === "client" && event.method === "thread/start",
    );
    assert.equal(
      start.params.config.mcp_servers.assay.command,
      identity.assayMcp.path,
      "thread/start must use the resolved Assay MCP binary, not an arbitrary command",
    );
    const classified = JSON.parse(
      fs.readFileSync(path.join(live.proofRoot, "classification.json"), "utf8"),
    );
    assert.equal(classified.externalAttestation, "not_provided");
    assert.notEqual(
      live.status,
      0,
      "a fake host-observation userAgent must keep the CLI nonzero",
    );

    const forgedRoot = scratch();
    for (const name of ["manifest.json", "events.json", "classification.json"]) {
      fs.copyFileSync(path.join(live.proofRoot, name), path.join(forgedRoot, name));
    }
    const forgedEvents = JSON.parse(fs.readFileSync(path.join(forgedRoot, "events.json"), "utf8"));
    const forged = JSON.parse(fs.readFileSync(path.join(forgedRoot, "manifest.json"), "utf8"));
    forged.captureMode = "host-observation";
    forged.hostIdentity = {
      os: "linux",
      arch: "x64",
      codex: {
        path: "/nonexistent/codex",
        version: "forged/1",
        sha256: "a".repeat(64),
        installSource: "self-attested",
      },
      assayMcp: {
        path: "/nonexistent/assay-mcp-server",
        version: "forged/1",
        sha256: "b".repeat(64),
        installSource: "self-attested",
      },
    };
    forged.hashes = { events: sha256Utf8(stableStringify(forgedEvents)) };
    const relabeled = classifyRecord({
      ...forged,
      events: forgedEvents,
      childExitCode: 0,
      driverOutcome: { exitCode: 0, status: "pass" },
      truncated: false,
      streamUnavailable: false,
    });
    assert.notEqual(
      relabeled.externalAttestation,
      "pass",
      "self-attested nonexistent binary paths must not mint a live pass",
    );
    fs.writeFileSync(path.join(forgedRoot, "manifest.json"), stableStringify(forged));
    fs.writeFileSync(path.join(forgedRoot, "classification.json"), stableStringify(relabeled));
    const checked = validateProofRoot(forgedRoot);
    assert.equal(checked.ok, false);
    assert.notEqual(checked.classified?.externalAttestation, "pass");

    const control = driveCli("valid", "discovery");
    assert.ok(control.stdout.includes("synthetic-fixture") || control.status !== undefined);
  } finally {
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("production spawn ignores user childArgv and rejects --mcp-command", async () => {
  const marker = path.join(scratch(), "spawned-from-child-argv");
  const evil = path.join(scratch(), "evil-child");
  writePortableNodeExecutable(
    evil,
    `import fs from "node:fs";
fs.writeFileSync(${JSON.stringify(marker)}, "spawned\\n");
`,
  );
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const shadow = writeShadowCodex([
    "node",
    FAKE,
    "--scenario",
    "valid",
    "--project-root",
    projectRoot,
  ]);
  const mcpBin = writeShadowMcp();
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(shadow)}${path.delimiter}${path.dirname(mcpBin)}${path.delimiter}${previousPath}`;
  try {
    await runProof({
      captureMode: "synthetic-fixture",
      timeoutMs: 4000,
      maxBytes: 1_048_576,
      journey: "discovery",
      allowLiveTurn: false,
      childArgv: [evil, "should-not-run"],
      proofRoot,
      projectRoot,
      mcpCommand: "/tmp/should-not-be-used-as-mcp",
    });
  } finally {
    process.env.PATH = previousPath;
  }
  assert.equal(
    fs.existsSync(marker),
    false,
    "user-provided childArgv must not reach production spawn",
  );
  const rejected = driveCli("valid", "discovery", {
    extraArgs: ["--mcp-command", "/tmp/arbitrary-mcp"],
  });
  assert.notEqual(rejected.status, 0, "production CLI must reject arbitrary --mcp-command");
  assert.match(`${rejected.stderr}${rejected.stdout}`, /mcp-command|unknown argument/i);
  const accepted = driveCli("valid", "discovery");
  assert.equal(typeof accepted.status, "number");
});

test("matching failed terminal turn dominates an earlier successful tool item", async () => {
  const { classified, driverOutcome, childExitCode } = await drive("tool-then-failed-turn");
  assert.equal(childExitCode, 0);
  assert.notEqual(
    classified.cells.oneToolInvoked.status,
    "pass",
    "failed turn/completed must dominate an earlier successful tool item",
  );
  assert.notEqual(driverOutcome.exitCode, 0, "dominated tool item must force nonzero driver exit");
  const control = await drive("valid");
  assert.equal(control.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(control.driverOutcome.exitCode, 0);
});

test("fresh proof directory and exclusive no-follow temps block predictable tmp symlink overwrite", async () => {
  const outside = path.join(os.tmpdir(), `assay-2684-sentinel-${process.pid}-${Date.now()}`);
  fs.writeFileSync(outside, "SAFE_SENTINEL\n");
  const projectRoot = seedProject();
  const proofRoot = scratch();
  fs.symlinkSync(outside, path.join(proofRoot, "manifest.json.tmp"));
  let threw = false;
  try {
    await runProof({
      captureMode: "synthetic-fixture",
      timeoutMs: 4000,
      maxBytes: 1_048_576,
      journey: "discovery",
      allowLiveTurn: false,
      childArgv: ["node", FAKE, "--scenario", "valid", "--project-root", projectRoot],
      proofRoot,
      projectRoot,
      assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
    });
  } catch {
    threw = true;
  }
  assert.equal(
    fs.readFileSync(outside, "utf8"),
    "SAFE_SENTINEL\n",
    "predictable dest.tmp symlink must not overwrite an outside file",
  );
  const nonempty = validateProofRoot(proofRoot);
  if (!threw) {
    assert.notEqual(
      nonempty.ok && fs.readFileSync(outside, "utf8") !== "SAFE_SENTINEL\n",
      true,
    );
  }
  const control = await drive("valid", "discovery");
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
  try {
    fs.unlinkSync(outside);
  } catch {
    /* leftover */
  }
});

test("elicitation accepts only the expected Assay request bound to the primary thread and turn", async () => {
  const wrongThread = await drive("elicit-wrong-thread");
  const replies = (events) =>
    events.filter(
      (event) =>
        event.direction === "client" && event.method === "mcpServer/elicitation/request",
    );
  assert.ok(replies(wrongThread.events).length >= 1);
  assert.equal(replies(wrongThread.events)[0].result.action, "decline");
  const nameOnly = await drive("elicit-assay-name-only");
  assert.equal(replies(nameOnly.events)[0].result.action, "decline");
  const control = await drive("valid");
  assert.equal(replies(control.events)[0].result.action, "accept");
});

test("one journey-to-required-cells function closes discovery, failures, tool, and unknown", async () => {
  const validator = await import("./codex_host_proof_validator.mjs");
  assert.equal(typeof validator.requiredCellsForJourney, "function");
  assert.deepEqual(validator.requiredCellsForJourney("discovery"), ["skillDiscovered"]);
  const failureCells = validator.requiredCellsForJourney("failures");
  assert.equal(failureCells.includes("oneToolInvoked"), false);
  assert.equal(failureCells.includes("structuredResultValidated"), false);
  assert.ok(failureCells.includes("missingBinaryNotClean"));
  assert.deepEqual(
    validator.requiredCellsForJourney("tool"),
    CELLS.filter((name) => name !== "driverCompleted"),
  );
  assert.throws(() => validator.requiredCellsForJourney("unknown"), /unknown journey/);
  await assert.rejects(
    () =>
      runProof({
        captureMode: "synthetic-fixture",
        timeoutMs: 4000,
        maxBytes: 1_048_576,
        journey: "unknown",
        allowLiveTurn: false,
        childArgv: ["node", FAKE, "--scenario", "valid", "--project-root", seedProject()],
        proofRoot: scratch(),
        projectRoot: seedProject(),
        assayMcpBin: "/tmp/x",
      }),
    /unknown journey/,
  );
  const missingDiscovery = await drive("missing-skill", "discovery");
  assert.notEqual(
    missingDiscovery.driverOutcome.exitCode,
    0,
    "discovery with missing skill must not inherit a clean child exit",
  );
  const failures = await drive("valid", "failures");
  assert.notEqual(failures.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(
    failures.driverOutcome.exitCode,
    0,
    "failures journey must be able to pass when its own cells pass",
  );
  const tool = await drive("valid", "tool");
  assert.equal(tool.driverOutcome.exitCode, 0);
  for (const name of validator.requiredCellsForJourney("tool")) {
    assert.equal(tool.classified.cells[name].status, "pass");
  }
});

test("forbidden root resolves existing ancestors and dir ceiling stops at entry 65", async () => {
  const isolatedHome = scratch();
  const codexHome = path.join(isolatedHome, ".codex");
  fs.mkdirSync(codexHome, { recursive: true });
  const linkRoot = scratch();
  const link = path.join(linkRoot, "link-to-codex");
  fs.symlinkSync(codexHome, link);
  const leaf = path.join(link, "nonexistent-leaf");
  const previousHome = process.env.HOME;
  process.env.HOME = isolatedHome;
  try {
    const reason = forbiddenProofRoot(leaf, "synthetic-fixture");
    assert.equal(typeof reason, "string");
    assert.match(reason, /CODEX_HOME|profile|auth|forbidden/i);
    fs.mkdirSync(leaf);
    const checked = validateProofRoot(leaf);
    assert.equal(checked.ok, false);
    assert.match(checked.reasons.join(" "), /CODEX_HOME|profile|auth|forbidden/i);
  } finally {
    if (previousHome === undefined) {
      delete process.env.HOME;
    } else {
      process.env.HOME = previousHome;
    }
  }
  const crowded = scratch();
  for (let i = 0; i < 80; i += 1) {
    fs.writeFileSync(path.join(crowded, `extra-${i}`), "x");
  }
  const started = Date.now();
  const listed = validateProofRoot(crowded);
  assert.ok(Date.now() - started < 2000, "directory ceiling must stop before materializing the full listing");
  assert.equal(listed.ok, false);
  assert.match(listed.reasons.join(" "), /exceeds bound|directory listing|unavailable/i);
  const control = forbiddenProofRoot(scratch(), "synthetic-fixture");
  assert.equal(control, null);
});

test("hosted CI catalogue runs the exact focused Node suite", () => {
  const command = "node --test --test-reporter spec scripts/ci/test_codex_host_proof.mjs";
  const ci = fs.readFileSync(path.join(HERE, "../../.github/workflows/ci.yml"), "utf8");
  const precommit = fs.readFileSync(path.join(HERE, "../../.pre-commit-config.yaml"), "utf8");
  const wired = [ci, precommit].some((text) => text.includes(command));
  assert.equal(
    wired,
    true,
    "removing the catalogue entry for the exact Node suite must go red",
  );
  const control = fs.readFileSync(path.join(HERE, "../../.github/workflows/ci.yml"), "utf8");
  assert.match(control, /bash scripts\/ci\/test-evidence-vocabulary\.sh/);
});

function writeMarkedBin(name, marker, version) {
  const bin = path.join(scratch(), name);
  writePortableNodeExecutable(
    bin,
    `import fs from "node:fs";
fs.writeFileSync(${JSON.stringify(marker)}, "ran\\n");
if (process.argv.includes("--version")) {
  process.stdout.write(${JSON.stringify(version)} + "\\n");
  process.exit(0);
}
process.stdin.resume();
`,
  );
  return bin;
}

test("production CLI refuses --codex-bin and --assay-mcp-bin before spawn", () => {
  assert.throws(() => parseArgs(["--codex-bin", "/tmp/not-a-codex"]), /unknown argument|--codex-bin/);
  assert.throws(
    () => parseArgs(["--assay-mcp-bin", "/tmp/not-an-mcp"]),
    /unknown argument|--assay-mcp-bin/,
  );
  const parsed = parseArgs(["--journey", "discovery"]);
  assert.equal(Object.hasOwn(parsed, "codexBin"), false);
  assert.equal(Object.hasOwn(parsed, "assayMcpBin"), false);

  const projectRoot = seedProject();
  const refuseFlag = (flag, binName, version) => {
    const marker = path.join(scratch(), `must-not-run-${binName}`);
    const evil = writeMarkedBin(binName, marker, version);
    const proofRoot = scratch();
    const cli = spawnSync(
      process.execPath,
      [
        path.join(HERE, "codex_host_proof.mjs"),
        "--capture-mode",
        "synthetic-fixture",
        "--proof-root",
        proofRoot,
        "--project-root",
        projectRoot,
        flag,
        evil,
        "--journey",
        "discovery",
        "--timeout-ms",
        "2000",
      ],
      { encoding: "utf8", timeout: 10_000 },
    );
    assert.notEqual(cli.status, 0, `production CLI must refuse ${flag}`);
    assert.match(`${cli.stderr}${cli.stdout}`, /unknown argument|codex-bin|assay-mcp-bin/i);
    assert.equal(
      fs.existsSync(marker),
      false,
      `refused ${flag} must not spawn or probe the provided path`,
    );
    assert.equal(
      fs.existsSync(path.join(proofRoot, "manifest.json")),
      false,
      `refused ${flag} must not write a proof pack`,
    );
  };
  refuseFlag("--codex-bin", "codex", "codex-flag/9.9.9");
  refuseFlag("--assay-mcp-bin", "assay-mcp-server", "assay-mcp-flag/9.9.9");
});

test("PATH shadows drive observed Codex and Assay MCP identities", () => {
  const projectRoot = seedProject();
  const mcpBin = writeShadowMcp();
  const codexBin = writeShadowCodex([
    "node",
    FAKE,
    "--scenario",
    "valid",
    "--project-root",
    projectRoot,
  ]);
  const flagCodex = writeMarkedBin("codex", path.join(scratch(), "flag-codex"), "codex-flag/9.9.9");
  const flagMcp = writeMarkedBin(
    "assay-mcp-server",
    path.join(scratch(), "flag-mcp"),
    "assay-mcp-flag/9.9.9",
  );
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(codexBin)}${path.delimiter}${path.dirname(mcpBin)}${path.delimiter}${previousPath}`;
  try {
    const ignored = resolveHostIdentity({
      codexBin: flagCodex,
      assayMcpBin: flagMcp,
    });
    assert.equal(ignored.codex.sha256, sha256File(codexBin));
    assert.equal(ignored.assayMcp.sha256, sha256File(mcpBin));
    assert.equal(sha256File(ignored.codex.path), sha256File(codexBin));
    assert.equal(sha256File(ignored.assayMcp.path), sha256File(mcpBin));
    assert.equal(ignored.codex.installSource, "PATH");
    assert.equal(ignored.assayMcp.installSource, "PATH");
    assert.match(ignored.codex.version, /codex-shadow/);
    assert.match(ignored.assayMcp.version, /assay-mcp-server-shadow/);
    assert.notEqual(ignored.codex.path, fs.realpathSync(flagCodex));
    assert.notEqual(ignored.assayMcp.path, fs.realpathSync(flagMcp));
  } finally {
    process.env.PATH = previousPath;
  }

  const proofRoot = scratch();
  const cli = spawnSync(
    process.execPath,
    [
      path.join(HERE, "codex_host_proof.mjs"),
      "--capture-mode",
      "synthetic-fixture",
      "--proof-root",
      proofRoot,
      "--project-root",
      projectRoot,
      "--journey",
      "tool",
      "--timeout-ms",
      "4000",
    ],
    {
      encoding: "utf8",
      timeout: 15_000,
      env: {
        ...process.env,
        PATH: `${path.dirname(codexBin)}${path.delimiter}${path.dirname(mcpBin)}${path.delimiter}${process.env.PATH}`,
      },
    },
  );
  assert.equal(typeof cli.status, "number");
  const manifest = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
  assert.equal(manifest.hostIdentity.codex.sha256, sha256File(codexBin));
  assert.equal(manifest.hostIdentity.assayMcp.sha256, sha256File(mcpBin));
  assert.equal(sha256File(manifest.hostIdentity.codex.path), sha256File(codexBin));
  assert.equal(sha256File(manifest.hostIdentity.assayMcp.path), sha256File(mcpBin));
  assert.deepEqual(Object.keys(manifest.hostIdentity).sort(), ["assayMcp", "codex"]);
  assert.deepEqual(Object.keys(manifest.hostIdentity.codex).sort(), ["path", "sha256"]);
  assert.deepEqual(Object.keys(manifest.hostIdentity.assayMcp).sort(), ["path", "sha256"]);
  const events = JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8"));
  const start = events.find(
    (event) => event.direction === "client" && event.method === "thread/start",
  );
  assert.equal(start.params.config.mcp_servers.assay.command, manifest.hostIdentity.assayMcp.path);
});

test("production selects PATH subjects once and executes proof-owned snapshots", async () => {
  const flagMarker = path.join(scratch(), "flag-codex-ran");
  const pathMarker = path.join(scratch(), "path-codex-ran");
  const flagCodex = writeMarkedBin("codex", flagMarker, "codex-flag/9.9.9");
  const pathDir = scratch();
  const pathCodex = path.join(pathDir, "codex");
  writePortableNodeExecutable(
    pathCodex,
    `import fs from "node:fs";
import { spawn } from "node:child_process";
fs.writeFileSync(${JSON.stringify(pathMarker)}, "ran\\n");
if (process.argv.includes("--version")) {
  process.stdout.write("codex-path/0.0.0\\n");
  process.exit(0);
}
const child = spawn(${JSON.stringify(process.execPath)}, ${JSON.stringify([FAKE, "--scenario", "valid", "--project-root", "unused"])}, { stdio: "inherit" });
const stop = () => {
  try { child.kill("SIGTERM"); } catch { /* already exited */ }
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
child.on("close", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
`,
  );
  const mcpBin = writeShadowMcp();
  const projectRoot = seedProject();
  const previousPath = process.env.PATH;
  process.env.PATH = `${pathDir}${path.delimiter}${path.dirname(mcpBin)}${path.delimiter}${previousPath}`;
  try {
    await runProof({
      captureMode: "synthetic-fixture",
      timeoutMs: 4000,
      maxBytes: 1_048_576,
      journey: "discovery",
      allowLiveTurn: false,
      codexBin: flagCodex,
      assayMcpBin: flagCodex,
      proofRoot: scratch(),
      projectRoot,
    });
  } finally {
    process.env.PATH = previousPath;
  }
  assert.equal(fs.existsSync(flagMarker), false, "options.codexBin must not be spawned or probed");
  assert.equal(fs.existsSync(pathMarker), true, "PATH shadow named codex must run");

});

test("swap-and-restore after identity hash cannot change the spawned Codex bytes", async () => {
  const projectRoot = seedProject();
  const aMarker = path.join(scratch(), "codex-snapshot-a-ran");
  const bMarker = path.join(scratch(), "codex-swapped-b-ran");
  const binDir = scratch();
  const codexPath = path.join(binDir, "codex");
  const writeMarkedShadow = (marker, version) => {
    writePortableNodeExecutable(
      codexPath,
      `import fs from "node:fs";
import { spawn } from "node:child_process";
fs.writeFileSync(${JSON.stringify(marker)}, "ran\\n");
if (process.argv.includes("--version")) {
  process.stdout.write(${JSON.stringify(version)} + "\\n");
  process.exit(0);
}
const child = spawn(${JSON.stringify(process.execPath)}, ${JSON.stringify([FAKE, "--scenario", "valid", "--project-root", projectRoot])}, { stdio: "inherit" });
const stop = () => {
  try { child.kill("SIGTERM"); } catch { /* already exited */ }
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
child.on("close", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
`,
    );
  };
  writeMarkedShadow(aMarker, "codex-bound-a/1.0.0");
  const mcpBin = writeShadowMcp();
  const previousPath = process.env.PATH;
  process.env.PATH = `${binDir}${path.delimiter}${path.dirname(mcpBin)}${path.delimiter}${previousPath}`;
  try {
    const identity = resolveHostIdentity();
    writeMarkedShadow(bMarker, "codex-swapped-b/9.9.9");
    const result = await runProof({
      captureMode: "synthetic-fixture",
      timeoutMs: 4000,
      maxBytes: 1_048_576,
      journey: "discovery",
      allowLiveTurn: false,
      hostIdentity: identity,
      proofRoot: scratch(),
      projectRoot,
    });
    assert.equal(fs.existsSync(bMarker), false, "swapped PATH binary must not be spawned or probed");
    assert.equal(fs.existsSync(aMarker), true, "hashed snapshot of the original binary must run");
    assert.equal(result.manifest.hostIdentity.codex.sha256, identity.codex.sha256);
    assert.notEqual(identity.codex.sha256, sha256File(codexPath));
  } finally {
    process.env.PATH = previousPath;
  }
  const control = await drive("valid", "discovery");
  assert.equal(control.classified.cells.skillDiscovered.status, "pass");
});

test("proof root inside child CODEX_HOME or event-derived home is rejected", async () => {
  const projectRoot = seedProject();
  const childHome = path.join(projectRoot, ".codex-home");
  fs.mkdirSync(childHome, { recursive: true });
  const leakedChild = spawnFakeChild(
    ["node", FAKE, "--scenario", "valid", "--project-root", projectRoot],
    projectRoot,
  );
  try {
    await assert.rejects(
      () =>
        runProof({
          captureMode: "synthetic-fixture",
          timeoutMs: 4000,
          maxBytes: 1_048_576,
          journey: "discovery",
          allowLiveTurn: false,
          testOnlyChild: leakedChild,
          proofRoot: childHome,
          projectRoot,
          assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
        }),
      /CODEX_HOME|runtime|profile|auth/i,
    );
  } finally {
    try {
      leakedChild.kill("SIGTERM");
    } catch {
      /* already exited */
    }
  }
  const eventHome = scratch();
  assert.equal(
    typeof forbiddenProofRoot(eventHome, "synthetic-fixture", [eventHome]),
    "string",
    "event-derived canonical Codex home must be a forbidden extra root",
  );
  const outside = scratch();
  assert.equal(
    forbiddenProofRoot(outside, "synthetic-fixture", [childHome, eventHome]),
    null,
    "a proof root outside configured and event-derived homes must stay accepted",
  );
  const control = await drive("valid", "discovery");
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
  assert.equal(
    forbiddenProofRoot(control.proofRoot, "synthetic-fixture", [
      path.join(control.projectRoot, ".codex-home"),
    ]),
    null,
  );
});

test("elicitation declines an unrelated same-turn Export profile data form", async () => {
  const exportProfile = {
    serverName: "assay",
    threadId: "thread-1",
    turnId: "turn-1",
    message: "Export profile data",
    mode: "form",
    requestedSchema: { type: "object", properties: { confirm: { type: "boolean" } } },
  };
  assert.equal(
    elicitationAcceptable(exportProfile, "thread-1", "turn-1"),
    false,
    "Export profile data must not be an acceptable Assay form",
  );
  const { events } = await drive("elicit-export-profile");
  const replies = events.filter(
    (event) =>
      event.direction === "client" && event.method === "mcpServer/elicitation/request",
  );
  assert.ok(replies.length >= 1, "export-profile elicitation must be recorded");
  assert.equal(replies[0].result.action, "decline");
  const control = await drive("valid");
  const accepted = control.events.filter(
    (event) =>
      event.direction === "client" && event.method === "mcpServer/elicitation/request",
  );
  assert.equal(accepted[0].result.action, "accept");
});

test("matching terminal turn must be completed; interrupted forces nonzero", async () => {
  const interrupted = await drive("tool-then-interrupted-turn");
  assert.equal(interrupted.childExitCode, 0);
  assert.notEqual(
    interrupted.classified.cells.oneToolInvoked.status,
    "pass",
    "interrupted turn/completed must not keep an earlier successful tool item",
  );
  assert.notEqual(
    interrupted.driverOutcome.exitCode,
    0,
    "completed→interrupted must force nonzero driver exit",
  );
  const { events, manifest, classified } = await drive("valid");
  assert.equal(classified.cells.oneToolInvoked.status, "pass");
  assert.equal(classified.driverOutcome?.exitCode ?? manifest.driverOutcome.exitCode, 0);
  const mutated = structuredClone({ ...manifest, events });
  const terminal = mutated.events.find(
    (event) => event.method === "turn/completed" && event.direction === "server",
  );
  assert.ok(terminal, "valid pack must have a matching turn/completed");
  terminal.params.turn.status = "interrupted";
  const relabeled = classifyRecord(mutated);
  assert.notEqual(
    relabeled.cells.oneToolInvoked.status,
    "pass",
    "classifyRecord completed→interrupted must fail oneToolInvoked",
  );
  const control = await drive("valid");
  assert.equal(control.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(control.driverOutcome.exitCode, 0);
});

function classifyMutated(manifest, events, mutateEvents) {
  const copy = structuredClone({ ...manifest, events });
  mutateEvents(copy.events);
  return classifyRecord(copy);
}

function elicitationRows(events, direction) {
  return events.filter(
    (event) =>
      event.method === "mcpServer/elicitation/request" && event.direction === direction,
  );
}

function serverMcpStatusRows(events) {
  return events.filter(
    (event) =>
      event.method === "mcpServerStatus/list" && event.direction === "server",
  );
}

function matchingTerminals(events, threadId, turnId) {
  return events.filter(
    (event) =>
      event.method === "turn/completed" &&
      event.direction === "server" &&
      event.params?.threadId === threadId &&
      event.params?.turn?.id === turnId,
  );
}

test("oneToolInvoked requires exactly one expected elicitation accept; zero/export/duplicate/declined fail closed", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(classified.cells.oneToolInvoked.status, "pass");
  for (const name of CELLS) {
    assert.equal(classified.cells[name].status, "pass", `${name} is the no-op control`);
  }
  const serverElicit = elicitationRows(events, "server");
  const clientElicit = elicitationRows(events, "client");
  assert.equal(serverElicit.length, 1);
  assert.equal(clientElicit.length, 1);
  assert.equal(clientElicit[0].result.action, "accept");
  assert.equal(
    elicitationAcceptable(
      serverElicit[0].params,
      serverElicit[0].params.threadId,
      serverElicit[0].params.turnId,
    ),
    true,
  );

  const zero = classifyMutated(manifest, events, (rows) => {
    for (let i = rows.length - 1; i >= 0; i -= 1) {
      if (rows[i].method === "mcpServer/elicitation/request") {
        rows.splice(i, 1);
      }
    }
  });
  assert.notEqual(
    zero.cells.oneToolInvoked.status,
    "pass",
    "zero elicitations must not pass oneToolInvoked",
  );

  const exportParams = {
    serverName: "assay",
    threadId: serverElicit[0].params.threadId,
    turnId: serverElicit[0].params.turnId,
    message: "Export profile data",
    mode: "form",
    requestedSchema: { type: "object", properties: { confirm: { type: "boolean" } } },
  };
  const exportProfile = classifyMutated(manifest, events, (rows) => {
    elicitationRows(rows, "server")[0].params = structuredClone(exportParams);
  });
  assert.notEqual(
    exportProfile.cells.oneToolInvoked.status,
    "pass",
    "unrelated accepted Export profile data must not pass oneToolInvoked",
  );
  assert.equal(
    elicitationAcceptable(
      exportParams,
      exportParams.threadId,
      exportParams.turnId,
    ),
    false,
    "Export profile data must stay declined by elicitationAcceptable",
  );

  const duplicates = classifyMutated(manifest, events, (rows) => {
    const request = structuredClone(elicitationRows(rows, "server")[0]);
    const accept = structuredClone(elicitationRows(rows, "client")[0]);
    request.id = "elicit-duplicate";
    accept.id = "elicit-duplicate";
    rows.push(request, accept);
  });
  assert.notEqual(
    duplicates.cells.oneToolInvoked.status,
    "pass",
    "duplicate accepted elicitations must not pass oneToolInvoked",
  );

  const declined = classifyMutated(manifest, events, (rows) => {
    elicitationRows(rows, "client")[0].result.action = "decline";
  });
  assert.notEqual(
    declined.cells.oneToolInvoked.status,
    "pass",
    "declined expected elicitation must not pass oneToolInvoked",
  );

  const control = await drive("valid");
  assert.equal(control.classified.cells.oneToolInvoked.status, "pass");
  for (const name of CELLS) {
    assert.equal(control.classified.cells[name].status, "pass");
  }
});

test("matching turn terminals require exactly one completed status; both orderings fail closed", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(classified.cells.oneToolInvoked.status, "pass");
  const completed = events.find(
    (event) =>
      event.method === "turn/completed" &&
      event.direction === "server" &&
      event.params?.turn?.status === "completed",
  );
  assert.ok(completed, "valid pack must have a completed turn/completed");
  const threadId = completed.params.threadId;
  const turnId = completed.params.turn.id;
  const failedTwin = structuredClone(completed);
  failedTwin.params.turn.status = "failed";

  const completedThenFailedEvents = structuredClone(events);
  completedThenFailedEvents.push(structuredClone(failedTwin));
  assert.deepEqual(
    matchingTerminals(completedThenFailedEvents, threadId, turnId).map(
      (event) => event.params.turn.status,
    ),
    ["completed", "failed"],
  );
  const completedThenFailed = classifyRecord({
    ...manifest,
    events: completedThenFailedEvents,
  });
  assert.notEqual(
    completedThenFailed.cells.oneToolInvoked.status,
    "pass",
    "completed→failed matching terminals must not pass oneToolInvoked",
  );

  const failedThenCompletedEvents = structuredClone(events);
  const insertAt = failedThenCompletedEvents.findIndex(
    (event) =>
      event.method === "turn/completed" &&
      event.direction === "server" &&
      event.params?.threadId === threadId &&
      event.params?.turn?.id === turnId,
  );
  failedThenCompletedEvents.splice(insertAt, 0, structuredClone(failedTwin));
  assert.deepEqual(
    matchingTerminals(failedThenCompletedEvents, threadId, turnId).map(
      (event) => event.params.turn.status,
    ),
    ["failed", "completed"],
  );
  const failedThenCompleted = classifyRecord({
    ...manifest,
    events: failedThenCompletedEvents,
  });
  assert.notEqual(
    failedThenCompleted.cells.oneToolInvoked.status,
    "pass",
    "failed→completed matching terminals must not pass oneToolInvoked",
  );

  const control = await drive("valid");
  assert.equal(control.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(control.driverOutcome.exitCode, 0);
});

test("duplicate Assay MCP-status rows fail closed in every consumer; no-op control stays green", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(classified.cells.mcpStarted.status, "pass");
  assert.equal(classified.cells.exactToolsListed.status, "pass");
  assert.equal(classified.cells.missingBinaryNotClean.status, "pass");
  assert.equal(classified.cells.invalidPolicyRootNotClean.status, "pass");
  const statuses = serverMcpStatusRows(events);
  assert.ok(statuses.length >= 3, "valid pack must have primary plus two negative status lists");
  const connected = structuredClone(statuses[0].result.data[0]);
  const failed = structuredClone(statuses[1].result.data[0]);
  assert.equal(connected.name, "assay");
  assert.equal(connected.runtimeStatus, "connected");
  assert.equal(failed.name, "assay");
  assert.equal(failed.runtimeStatus, "failed");

  const failedThenConnected = classifyMutated(manifest, events, (rows) => {
    serverMcpStatusRows(rows)[1].result.data = [
      structuredClone(failed),
      structuredClone(connected),
    ];
  });
  assert.notEqual(
    failedThenConnected.cells.missingBinaryNotClean.status,
    "pass",
    "failed/0-tools plus connected/5-tools must not pass the negative cell",
  );

  const connectedThenFailed = classifyMutated(manifest, events, (rows) => {
    serverMcpStatusRows(rows)[1].result.data = [
      structuredClone(connected),
      structuredClone(failed),
    ];
  });
  assert.notEqual(
    connectedThenFailed.cells.missingBinaryNotClean.status,
    "pass",
    "connected/5-tools plus failed/0-tools must not pass the negative cell",
  );

  const duplicatePrimary = classifyMutated(manifest, events, (rows) => {
    const primary = serverMcpStatusRows(rows)[0];
    primary.result.data = [structuredClone(connected), structuredClone(failed)];
  });
  assert.notEqual(
    duplicatePrimary.cells.mcpStarted.status,
    "pass",
    "duplicate Assay rows must fail closed in classifyMcp",
  );
  assert.notEqual(
    duplicatePrimary.cells.exactToolsListed.status,
    "pass",
    "duplicate Assay rows must fail closed in classifyTools",
  );

  const control = await drive("valid");
  assert.equal(control.classified.cells.mcpStarted.status, "pass");
  assert.equal(control.classified.cells.exactToolsListed.status, "pass");
  assert.equal(control.classified.cells.missingBinaryNotClean.status, "pass");
  assert.equal(control.classified.cells.invalidPolicyRootNotClean.status, "pass");
});

function allIntendedCellsPass(classified) {
  return CELLS.every((name) => classified.cells[name].status === "pass");
}

function rewriteProof(proofRoot, manifest, events, classified) {
  const next = structuredClone(manifest);
  next.hashes = { events: sha256Utf8(stableStringify(events)) };
  fs.writeFileSync(path.join(proofRoot, "manifest.json"), stableStringify(next));
  fs.writeFileSync(path.join(proofRoot, "events.json"), stableStringify(events));
  fs.writeFileSync(path.join(proofRoot, "classification.json"), stableStringify(classified));
  return next;
}

function clientThreadStarts(events) {
  return events.filter(
    (event) => event.direction === "client" && event.method === "thread/start",
  );
}

test("F1 strict JSON-RPC envelopes reject method+result, result+error, duplicate, and unknown ids; control stays green", async () => {
  const impersonated = await drive("notification-impersonates-response");
  assert.equal(
    allIntendedCellsPass(impersonated.classified),
    false,
    "a notification carrying a pending id must not yield all-pass",
  );
  assert.notEqual(impersonated.classified.cells.driverCompleted.status, "pass");
  assert.notEqual(
    impersonated.manifest.driverOutcome.exitCode,
    0,
    "a rejected envelope must be retained as a non-clean driver outcome",
  );
  assert.equal(
    validateProofRoot(impersonated.proofRoot).ok,
    true,
    "honest evidence of a failed journey remains a valid proof record",
  );

  const both = await drive("result-and-error-response");
  assert.equal(
    allIntendedCellsPass(both.classified),
    false,
    "result+error must not resolve a pending id as all-pass",
  );
  assert.notEqual(both.classified.cells.driverCompleted.status, "pass");

  const duplicate = await drive("duplicate-initialize-response");
  assert.equal(
    allIntendedCellsPass(duplicate.classified),
    false,
    "a duplicate response id must not yield all-pass",
  );
  assert.notEqual(duplicate.classified.cells.driverCompleted.status, "pass");

  const unknown = await drive("unknown-response-id");
  assert.equal(
    allIntendedCellsPass(unknown.classified),
    false,
    "an unknown response id must not yield all-pass",
  );
  assert.notEqual(unknown.classified.cells.driverCompleted.status, "pass");

  assert.match(DRIVER_SRC, /resolvePendingResponse/);
  assert.match(VALIDATOR_SRC, /export function resolvePendingResponse/);

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("F2 closed driver-outcome rule rejects child/driver contradictions; control stays green", async () => {
  const { events, manifest, classified, proofRoot } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);

  const deadChild = classifyRecord({
    ...manifest,
    events,
    childExitCode: 9,
    driverOutcome: { exitCode: 0, status: "pass" },
  });
  assert.notEqual(
    deadChild.cells.driverCompleted.status,
    "pass",
    "childExitCode 9 plus driver pass/0 must not pass",
  );
  rewriteProof(
    proofRoot,
    { ...manifest, childExitCode: 9, driverOutcome: { exitCode: 0, status: "pass" } },
    events,
    deadChild,
  );
  assert.equal(
    validateProofRoot(proofRoot).ok,
    false,
    "contradictory child 9 + driver pass must not validate cleanly",
  );

  for (const status of ["fail", "garbage", undefined]) {
    const outcome =
      status === undefined ? { exitCode: 0 } : { exitCode: 0, status };
    const zeroExit = classifyRecord({
      ...manifest,
      events,
      childExitCode: 0,
      driverOutcome: outcome,
    });
    assert.notEqual(
      zeroExit.cells.driverCompleted.status,
      "pass",
      `exit 0 with status ${String(status)} must not pass`,
    );
  }

  const missing = classifyRecord({
    ...manifest,
    events,
    childExitCode: 0,
    driverOutcome: null,
  });
  assert.equal(
    missing.cells.driverCompleted.status,
    "pass",
    "preliminary null outcome plus child 0 remains derivable",
  );
  const missingRoot = scratch();
  fs.mkdirSync(missingRoot, { recursive: true });
  rewriteProof(
    missingRoot,
    { ...manifest, childExitCode: 0, driverOutcome: null },
    events,
    missing,
  );
  assert.equal(
    validateProofRoot(missingRoot).ok,
    false,
    "stored null driverOutcome must not validate as a pass pack",
  );

  assert.match(DRIVER_SRC, /closedDriverOutcomeStatus/);
  assert.match(VALIDATOR_SRC, /export function closedDriverOutcomeStatus/);

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(control.childExitCode, 0);
  assert.equal(control.driverOutcome.exitCode, 0);
  assert.equal(control.driverOutcome.status, "pass");
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("F3 canonical tool contract rejects pack-mutated tools, unlisted_probe, and isError; control stays green", async () => {
  const { events, manifest, classified, proofRoot } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  assert.deepEqual(manifest.expected.tools, [...EXPECTED_TOOLS]);
  assert.equal(manifest.expected.toolName, DECIDE_TOOL);
  assert.deepEqual(manifest.expected.toolArguments, DECIDE_INPUT);

  const hostile = classifyMutated(
    {
      ...manifest,
      expected: {
        ...manifest.expected,
        tools: [...EXPECTED_TOOLS, "unlisted_probe"],
        toolName: "unlisted_probe",
        toolArguments: { probe: true },
      },
    },
    events,
    (rows) => {
      const call = toolCompleted(rows);
      call.params.item.tool = "unlisted_probe";
      call.params.item.arguments = { probe: true };
    },
  );
  assert.notEqual(
    hostile.cells.oneToolInvoked.status,
    "pass",
    "pack-mutated expected tools plus unlisted_probe must not pass invocation",
  );
  assert.notEqual(hostile.cells.structuredResultValidated.status, "pass");

  const errorFlag = classifyMutated(manifest, events, (rows) => {
    toolCompleted(rows).params.item.result.isError = true;
  });
  assert.notEqual(
    errorFlag.cells.structuredResultValidated.status,
    "pass",
    "isError:true must not pass structured result validation",
  );

  const errorBearing = classifyMutated(manifest, events, (rows) => {
    toolCompleted(rows).params.item.result.error = { message: "tool exploded" };
  });
  assert.notEqual(
    errorBearing.cells.structuredResultValidated.status,
    "pass",
    "an error-bearing MCP result must not pass",
  );

  const hostileEvents = structuredClone(events);
  toolCompleted(hostileEvents).params.item.tool = "unlisted_probe";
  toolCompleted(hostileEvents).params.item.arguments = { probe: true };
  const hostileManifest = {
    ...manifest,
    expected: {
      ...manifest.expected,
      tools: [...EXPECTED_TOOLS, "unlisted_probe"],
      toolName: "unlisted_probe",
      toolArguments: { probe: true },
    },
  };
  rewriteProof(proofRoot, hostileManifest, hostileEvents, hostile);
  assert.equal(
    validateProofRoot(proofRoot).ok,
    false,
    "a pack that rewrites the expected tool set must not validate",
  );

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(control.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(control.classified.cells.structuredResultValidated.status, "pass");
  assert.equal(control.manifest.expected.projectRoot, control.projectRoot);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("F4 id-pair topology rejects omitted status requests, extra contradictory rows, and unbound responses; control stays green", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  assert.equal(clientParams(events, "mcpServerStatus/list").length, 3);
  assert.equal(clientThreadStarts(events).length, 3);

  const stripped = classifyMutated(manifest, events, (rows) => {
    for (let i = rows.length - 1; i >= 0; i -= 1) {
      const row = rows[i];
      if (row.method === "mcpServerStatus/list" && row.direction === "client") {
        rows.splice(i, 1);
        continue;
      }
      if (row.method !== "thread/start") {
        continue;
      }
      const command = row.params?.config?.mcp_servers?.assay?.command ?? "";
      const args = row.params?.config?.mcp_servers?.assay?.args ?? [];
      const negative =
        String(command).includes("missing-assay-mcp-server") ||
        args.some((value) => String(value).includes("missing-policy-root"));
      if (negative) {
        rows.splice(i, 1);
      }
    }
  });
  assert.notEqual(
    stripped.cells.mcpStarted.status,
    "pass",
    "removing status requests and negative thread pairs must not keep MCP cells passing",
  );
  assert.notEqual(stripped.cells.missingBinaryNotClean.status, "pass");
  assert.notEqual(stripped.cells.invalidPolicyRootNotClean.status, "pass");

  const extraStatus = classifyMutated(manifest, events, (rows) => {
    rows.push({
      direction: "server",
      method: "mcpServerStatus/list",
      id: 99,
      result: { data: [{ name: "assay", runtimeStatus: "failed", tools: {} }] },
    });
  });
  assert.notEqual(
    extraStatus.cells.mcpStarted.status,
    "pass",
    "a fourth contradictory status response must fail closed",
  );

  const extraSkill = classifyMutated(manifest, events, (rows) => {
    rows.push({
      direction: "server",
      method: "skills/list",
      id: 98,
      result: { data: [{ cwd: manifest.expected.projectRoot, errors: [], skills: [] }] },
    });
  });
  assert.notEqual(
    extraSkill.cells.skillDiscovered.status,
    "pass",
    "an extra contradictory skills response must fail closed",
  );

  const duplicateSkillRow = classifyMutated(manifest, events, (rows) => {
    const listed = rows.find(
      (event) => event.method === "skills/list" && event.direction === "server",
    );
    listed.result.data[0].skills.push({
      name: "assay-golden-path",
      enabled: false,
      path: path.join(manifest.expected.projectRoot, "other", "SKILL.md"),
    });
  });
  assert.notEqual(
    duplicateSkillRow.cells.skillDiscovered.status,
    "pass",
    "a duplicate contradictory skills row must fail closed",
  );

  assert.match(DRIVER_SRC, /consumeJourneyTopology/);
  assert.match(VALIDATOR_SRC, /export function consumeJourneyTopology/);

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(control.classified.cells.mcpStarted.status, "pass");
  assert.equal(control.classified.cells.skillDiscovered.status, "pass");
  const discovery = await drive("valid", "discovery");
  assert.equal(discovery.classified.cells.skillDiscovered.status, "pass");
  assert.notEqual(discovery.classified.cells.mcpStarted.status, "pass");
});

function writeVersionOnlyBin(name, version) {
  const bin = path.join(scratch(), name);
  writePortableNodeExecutable(
    bin,
    `if (process.argv.includes("--version")) {
  process.stdout.write(${JSON.stringify(version)} + "\\n");
  process.exit(0);
}
`,
  );
  return bin;
}

test("version observation rejects output from a failed probe", () => {
  const codex = path.join(scratch(), "codex");
  writePortableNodeExecutable(
    codex,
    `if (process.argv.includes("--version")) {
  process.stdout.write("codex-failed/0.0.0\\n");
  process.exit(1);
}
`,
  );
  const mcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-control/0.0.0");
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(codex)}${path.delimiter}${path.dirname(mcp)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(
      () => resolveHostIdentity(),
      /version probe failed|nonzero|exit/i,
      "version text from a failed executable must not become observed identity",
    );
  } finally {
    process.env.PATH = previousPath;
  }
});

test("host snapshots require a private proof root owned by the current user", {
  skip: process.platform === "win32",
}, () => {
  const proofRoot = scratch();
  const codex = writeVersionOnlyBin("codex", "codex-control/0.0.0");
  const mcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-control/0.0.0");
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(codex)}${path.delimiter}${path.dirname(mcp)}${path.delimiter}${previousPath}`;
  try {
    for (const mode of [0o777, 0o500]) {
      fs.chmodSync(proofRoot, mode);
      assert.throws(
        () => resolveHostIdentity({ proofRoot }),
        /proof root must be private to its owner \(mode 0700\)/i,
        `proof-root mode ${mode.toString(8)} must fail before a snapshot executes`,
      );
    }
  } finally {
    process.env.PATH = previousPath;
    fs.chmodSync(proofRoot, 0o700);
  }
});

test("validator rejects a proof root that is no longer private", {
  skip: process.platform === "win32",
}, async () => {
  const { proofRoot } = await drive("valid");
  assert.equal(validateProofRoot(proofRoot).ok, true, "private control must validate");
  try {
    for (const mode of [0o755, 0o500]) {
      fs.chmodSync(proofRoot, mode);
      const checked = validateProofRoot(proofRoot);
      assert.equal(checked.ok, false, `proof-root mode ${mode.toString(8)} must not validate`);
      assert.match(checked.reasons.join(" "), /private|owner|permission|mode/i);
    }
  } finally {
    fs.chmodSync(proofRoot, 0o700);
  }
});

test("production CLI canonicalizes the macOS tmp alias before binding host subjects", {
  skip:
    process.platform === "win32" ||
    !fs.existsSync("/tmp") ||
    fs.realpathSync("/tmp") === path.resolve("/tmp"),
}, () => {
  const proofRoot = fs.mkdtempSync(path.join("/tmp", "assay-2684-alias-"));
  const result = driveCli("valid", "discovery", { proofRoot });
  assert.equal(
    result.status,
    0,
    `a proof written through /tmp must validate through its canonical path: ${result.stderr}`,
  );
  assert.equal(validateProofRoot(proofRoot).ok, true);
});

test("F5 PATH snapshot copy enforces a binary ceiling and running growth bound; control stays green", () => {
  assert.equal(HARD_MAX_SNAPSHOT_BYTES, 536870912, "documented 512 MiB per-binary ceiling");
  assert.match(DRIVER_SRC, /HARD_MAX_SNAPSHOT_BYTES/);
  const previousPath = process.env.PATH;
  const oversizedCodex = writeVersionOnlyBin("codex", "codex-oversize/0.0.0");
  const oversizedMcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-oversize/0.0.0");
  assert.ok(fs.statSync(oversizedCodex).size > 16, "oversize fixture stays tiny and valid");
  process.env.PATH = `${path.dirname(oversizedCodex)}${path.delimiter}${path.dirname(oversizedMcp)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(
      () => resolveHostIdentity({ testOnlySnapshotMaxBytes: 16 }),
      /ceiling|exceeds|snapshot/i,
      "a valid PATH binary above the injected ceiling must fail before materialization",
    );
  } finally {
    process.env.PATH = previousPath;
  }

  const growCodex = writeVersionOnlyBin("codex", "codex-grow/0.0.0");
  const growMcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-grow/0.0.0");
  const growSize = fs.statSync(growCodex).size;
  process.env.PATH = `${path.dirname(growCodex)}${path.delimiter}${path.dirname(growMcp)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(
      () =>
        resolveHostIdentity({
          testOnlySnapshotMaxBytes: growSize,
          testOnlyAfterSnapshotRead(copied, src, destName) {
            if (destName === "codex" && copied === 0) {
              fs.appendFileSync(src, `\n//${"y".repeat(32)}`);
            }
          },
        }),
      /ceiling|grew|exceeded|snapshot/i,
      "a snapshot that grows past the ceiling while copying must fail",
    );
  } finally {
    process.env.PATH = previousPath;
  }

  const controlCodex = writeShadowCodex([
    "node",
    FAKE,
    "--scenario",
    "valid",
    "--project-root",
    seedProject(),
  ]);
  const controlMcp = writeShadowMcp();
  process.env.PATH = `${path.dirname(controlCodex)}${path.delimiter}${path.dirname(controlMcp)}${path.delimiter}${previousPath}`;
  try {
    const control = resolveHostIdentity();
    assert.equal(control.codex.installSource, "PATH");
    assert.equal(control.assayMcp.installSource, "PATH");
    assert.match(control.codex.version, /codex-shadow/);
  } finally {
    process.env.PATH = previousPath;
  }
});

function writeSparseFile(filePath, size) {
  const fd = fs.openSync(filePath, "w", 0o755);
  try {
    fs.ftruncateSync(fd, size);
  } finally {
    fs.closeSync(fd);
  }
  fs.chmodSync(filePath, 0o755);
  return filePath;
}

function writeSparseBin(name, size) {
  return writeSparseFile(path.join(scratch(), name), size);
}

function snapRoots() {
  return fs.readdirSync(os.tmpdir()).filter((name) => name.startsWith("assay-host-snap-"));
}

function missingBinaryThreadId(events) {
  const request = events.find(
    (event) =>
      event.direction === "client" &&
      event.method === "thread/start" &&
      String(event.params?.config?.mcp_servers?.assay?.command ?? "").includes(
        "missing-assay-mcp-server",
      ),
  );
  const response = events.find(
    (event) => event.direction === "server" && event.id === request?.id,
  );
  return response?.result?.thread?.id;
}

function liveBoundRecord(manifest, events) {
  const previousPath = process.env.PATH;
  const mcp = writeShadowMcp();
  const projectRoot = seedProject();
  const codex = writeShadowCodex([
    "node",
    FAKE,
    "--scenario",
    "valid",
    "--project-root",
    projectRoot,
  ]);
  process.env.PATH = `${path.dirname(codex)}${path.delimiter}${path.dirname(mcp)}${path.delimiter}${previousPath}`;
  try {
    const identity = resolveHostIdentity();
    const patched = structuredClone(events);
    const start = patched.find(
      (event) => event.direction === "client" && event.method === "thread/start",
    );
    start.params.config.mcp_servers.assay.command = identity.assayMcp.path;
    return {
      ...manifest,
      captureMode: "host-observation",
      events: patched,
      hostIdentity: identity,
      invocation: {
        argv: [identity.codex.path, "app-server"],
        envNames: ["PATH", "HOME", "CODEX_HOME"],
      },
      childExitCode: 0,
      driverOutcome: { exitCode: 0, status: "pass" },
      truncated: false,
      streamUnavailable: false,
    };
  } finally {
    process.env.PATH = previousPath;
  }
}

test("P1 stored response envelopes cannot ignore method-less server rows; control stays green", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);

  const hostiles = [
    { direction: "server", id: 999, result: { unexpected: true } },
    { direction: "server", id: 998, result: { ok: true }, error: { code: -1 } },
    { direction: "server", id: 997 },
    { direction: "server", id: 1, method: "initialize", result: { replay: true } },
  ];
  for (const row of hostiles) {
    const mutated = structuredClone(events);
    mutated.push(row);
    const hostile = classifyRecord({ ...manifest, events: mutated });
    assert.equal(
      allIntendedCellsPass(hostile),
      false,
      `${JSON.stringify(row)} must not yield all-pass`,
    );
    const hostileRoot = scratch();
    fs.mkdirSync(hostileRoot, { recursive: true });
    rewriteProof(hostileRoot, manifest, mutated, hostile);
    assert.equal(
      validateProofRoot(hostileRoot).ok,
      false,
      `${JSON.stringify(row)} must not validate as proof`,
    );
  }

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("P1 paired topology binds initialize and turn/start; control stays green", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  const bound = liveBoundRecord(manifest, events);
  assert.equal(
    classifyRecord(bound).externalAttestation,
    "not_provided",
    "no-op live-bound control still cannot pass on fake events",
  );

  const hidden = structuredClone(bound);
  hidden.events.unshift({ direction: "server", method: "initialize" });
  const hid = classifyRecord(hidden);
  assert.notEqual(
    hid.externalAttestation,
    "pass",
    "a prepended method-only initialize must not hide the paired fake user-agent",
  );

  const missingThread = missingBinaryThreadId(events);
  assert.equal(typeof missingThread, "string");
  const rebound = classifyMutated(manifest, events, (rows) => {
    const turn = rows.find(
      (event) => event.direction === "client" && event.method === "turn/start",
    );
    turn.params.threadId = missingThread;
  });
  assert.equal(
    allIntendedCellsPass(rebound),
    false,
    "turn/start rebound to the missing-binary thread must not stay all-pass",
  );
  const reboundRoot = scratch();
  fs.mkdirSync(reboundRoot, { recursive: true });
  const reboundEvents = structuredClone(events);
  reboundEvents.find(
    (event) => event.direction === "client" && event.method === "turn/start",
  ).params.threadId = missingThread;
  rewriteProof(reboundRoot, manifest, reboundEvents, rebound);
  assert.equal(
    validateProofRoot(reboundRoot).ok,
    false,
    "turn/start not on the primary thread must not validate as proof",
  );

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("P1 shared 512 MiB ceiling rejects sparse hash and removes snap root; control stays green", () => {
  const huge = writeSparseFile(path.join(scratch(), "identity.bin"), 536870912 + 1);
  const origRead = fs.readSync;
  let readCalls = 0;
  fs.readSync = function readSyncGuarded(...args) {
    readCalls += 1;
    return origRead.apply(this, args);
  };
  try {
    assert.throws(
      () => sha256File(huge),
      /ceiling|exceeds|binary/,
      "validator hash must reject a sparse 512 MiB+1 file before reading it",
    );
    assert.equal(
      readCalls,
      0,
      "production-default sparse 512 MiB+1 must not call readSync; initial fstat ceiling must reject first",
    );
  } finally {
    fs.readSync = origRead;
  }

  const previousPath = process.env.PATH;
  const before = new Set(snapRoots());
  const small = writeVersionOnlyBin("codex", "codex-first/0.0.0");
  const second = writeSparseBin("assay-mcp-server", 536870912 + 1);
  process.env.PATH = `${path.dirname(small)}${path.delimiter}${path.dirname(second)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(
      () => resolveHostIdentity(),
      /ceiling|exceeds|snapshot|binary/,
      "second-binary ceiling failure must throw",
    );
    const leftovers = snapRoots().filter((name) => !before.has(name));
    assert.deepEqual(
      leftovers,
      [],
      "second-binary failure must remove the whole assay-host-snap root including the first snapshot",
    );
  } finally {
    process.env.PATH = previousPath;
  }

  const controlCodex = writeShadowCodex([
    "node",
    FAKE,
    "--scenario",
    "valid",
    "--project-root",
    seedProject(),
  ]);
  const controlMcp = writeShadowMcp();
  process.env.PATH = `${path.dirname(controlCodex)}${path.delimiter}${path.dirname(controlMcp)}${path.delimiter}${previousPath}`;
  try {
    const control = resolveHostIdentity();
    assert.equal(control.codex.installSource, "PATH");
    assert.equal(control.assayMcp.installSource, "PATH");
  } finally {
    process.env.PATH = previousPath;
  }
});

test("P1 snapshot and decide-tool guards bite independently of overrides; control stays green", async () => {
  const fakeSrc = fs.readFileSync(FAKE, "utf8");
  assert.doesNotMatch(
    fakeSrc,
    /\bDECIDE_TOOL\b/,
    "fake app-server must pin the release decide tool independently of DECIDE_TOOL",
  );
  assert.match(fakeSrc, /"assay_policy_decide"/);
  assert.equal(DECIDE_TOOL, "assay_policy_decide");

  const previousPath = process.env.PATH;
  const oversized = writeSparseBin("codex", 536870912 + 1);
  const mcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-prod/0.0.0");
  process.env.PATH = `${path.dirname(oversized)}${path.delimiter}${path.dirname(mcp)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(
      () =>
        resolveHostIdentity({
          testOnlyAfterSnapshotRead() {
            throw new Error("must not materialize a production-default oversize PATH file");
          },
        }),
      /ceiling|exceeds|snapshot|binary/,
      "production-default 512 MiB ceiling must reject sparse 512 MiB+1 with no override",
    );
  } finally {
    process.env.PATH = previousPath;
  }

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(control.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

function pairedInitialize(events) {
  return events.find(
    (event) =>
      event.direction === "server" && event.method === "initialize" && event.id != null,
  );
}

function pairedPrimaryClientStart(events) {
  return events.find(
    (event) =>
      event.direction === "client" &&
      event.method === "thread/start" &&
      event.id != null &&
      !String(event.params?.config?.mcp_servers?.assay?.command ?? "").includes(
        "missing-assay-mcp-server",
      ) &&
      !(event.params?.config?.mcp_servers?.assay?.args ?? []).some((value) =>
        String(value).includes("missing-policy-root"),
      ),
  );
}

test("closed-world server-row taxonomy refuses id-less response shapes; allowed rows stay green", async () => {
  const { events, manifest, classified, proofRoot } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  assert.equal(validateProofRoot(proofRoot).ok, true);
  const allowed = {
    elicitation: events.filter(
      (event) =>
        event.direction === "server" && event.method === "mcpServer/elicitation/request",
    ),
    items: events.filter(
      (event) => event.direction === "server" && event.method === "item/completed",
    ),
    turns: events.filter(
      (event) => event.direction === "server" && event.method === "turn/completed",
    ),
    initialize: events.filter(
      (event) =>
        event.direction === "server" &&
        event.method === "initialize" &&
        event.id != null &&
        Object.prototype.hasOwnProperty.call(event, "result") &&
        !Object.prototype.hasOwnProperty.call(event, "error"),
    ),
  };
  assert.equal(allowed.elicitation.length, 1, "pin allowed elicitation request");
  assert.ok(allowed.items.length >= 1, "pin allowed item/completed notification");
  assert.equal(allowed.turns.length, 1, "pin allowed turn/completed notification");
  assert.equal(allowed.initialize.length, 1, "pin allowed paired initialize result");

  const hostiles = [
    { direction: "server", result: { unexpected: true } },
    { direction: "server", error: { code: -32000, message: "id-less error" } },
    { direction: "server", result: { ok: true }, error: { code: -1 } },
    { direction: "server", method: "initialize", result: { replay: true } },
  ];
  for (const row of hostiles) {
    const mutated = structuredClone(events);
    mutated.push(row);
    const hostile = classifyRecord({ ...manifest, events: mutated });
    assert.equal(
      allIntendedCellsPass(hostile),
      false,
      `${JSON.stringify(row)} must not yield 9/9`,
    );
    const hostileRoot = scratch();
    fs.mkdirSync(hostileRoot, { recursive: true });
    rewriteProof(hostileRoot, manifest, mutated, hostile);
    assert.equal(
      validateProofRoot(hostileRoot).ok,
      false,
      `${JSON.stringify(row)} must not validate as proof`,
    );
  }

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("current Codex lifecycle chatter is inert while diagnostics remain non-clean", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);

  const lifecycleMethods = [
    "remoteControl/status/changed",
    "thread/started",
    "mcpServer/startupStatus/updated",
    "thread/status/changed",
    "turn/started",
    "item/started",
  ];
  const withLifecycle = structuredClone(events);
  const terminalIndex = withLifecycle.findIndex(
    (event) => event.direction === "server" && event.method === "turn/completed",
  );
  assert.notEqual(terminalIndex, -1, "control must contain the terminal turn insertion anchor");
  withLifecycle.splice(
    terminalIndex,
    0,
    ...lifecycleMethods.map((method) => ({ direction: "server", method, params: {} })),
  );
  const lifecycle = classifyRecord({ ...manifest, events: withLifecycle });
  assert.equal(
    allIntendedCellsPass(lifecycle),
    true,
    "known lifecycle notifications must not erase independently observed host cells",
  );
  const lifecycleRoot = scratch();
  fs.mkdirSync(lifecycleRoot, { recursive: true });
  rewriteProof(lifecycleRoot, manifest, withLifecycle, lifecycle);
  assert.equal(validateProofRoot(lifecycleRoot).ok, true);

  for (const method of ["warning", "error"]) {
    const withDiagnostic = structuredClone(events);
    withDiagnostic.splice(terminalIndex, 0, { direction: "server", method, params: {} });
    const diagnostic = classifyRecord({ ...manifest, events: withDiagnostic });
    assert.equal(
      diagnostic.cells.skillDiscovered.status,
      "pass",
      `${method} must not relabel completed discovery as absent`,
    );
    assert.equal(
      diagnostic.cells.exactToolsListed.status,
      "pass",
      `${method} must not relabel the observed tool list as absent`,
    );
    assert.equal(
      diagnostic.cells.driverCompleted.status,
      "fail",
      `${method} must keep the overall proof non-clean`,
    );
    assert.notEqual(
      driverOutcomeFrom(
        { childExit: 0, truncated: false, streamUnavailable: false },
        diagnostic.cells,
        "tool",
      ).exitCode,
      0,
      `${method} must keep the generated driver outcome nonzero`,
    );
    assert.equal(allIntendedCellsPass(diagnostic), false);
  }

  const malformedLifecycle = structuredClone(events);
  malformedLifecycle.splice(terminalIndex, 0, {
    direction: "server",
    method: lifecycleMethods[0],
    params: { unexpected: true },
  });
  assert.equal(
    allIntendedCellsPass(classifyRecord({ ...manifest, events: malformedLifecycle })),
    false,
    "lifecycle payloads outside the retained empty projection must fail closed",
  );

  const unknown = structuredClone(events);
  unknown.splice(terminalIndex, 0, {
    direction: "server",
    method: "future/unknown",
    params: {},
  });
  assert.equal(
    allIntendedCellsPass(classifyRecord({ ...manifest, events: unknown })),
    false,
    "future notification methods remain fail-closed",
  );
});

test("successful initialize result is required; initialize error cannot clean the record", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  const mutated = structuredClone(events);
  const initialize = pairedInitialize(mutated);
  assert.ok(initialize, "valid pack must have a paired initialize result");
  delete initialize.result;
  initialize.error = { code: -32000, message: "initialize failed" };

  const cellsOnly = classifyRecord({ ...manifest, events: mutated });
  assert.equal(
    allIntendedCellsPass(cellsOnly),
    false,
    "a uniquely paired initialize JSON-RPC error must not keep 9/9",
  );

  const bound = liveBoundRecord(manifest, mutated);
  const live = classifyRecord(bound);
  assert.notEqual(
    live.externalAttestation,
    "pass",
    "initialize error converted to an empty record must not invent attestation",
  );
  const errorRoot = scratch();
  fs.mkdirSync(errorRoot, { recursive: true });
  rewriteProof(errorRoot, bound, bound.events, live);
  assert.equal(
    validateProofRoot(errorRoot).ok,
    false,
    "initialize error pack must not validate as proof",
  );

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(
    classifyRecord(liveBoundRecord(control.manifest, control.events)).externalAttestation,
    "not_provided",
    "no-op live-bound control still cannot pass on fake events",
  );
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("canonical topology and live identity both reject a wrong primary command", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  const bound = liveBoundRecord(manifest, events);
  const initialize = pairedInitialize(bound.events);
  initialize.result.userAgent = "codex_cli/0.50.0";
  const observed = bound.hostIdentity.assayMcp.path;
  const primary = pairedPrimaryClientStart(bound.events);
  assert.ok(primary, "valid pack must have a paired primary thread/start");
  assert.equal(primary.params.config.mcp_servers.assay.command, observed);
  // Keep every RPC envelope valid. The missing/invalid thread/start rows stay
  // as decoys; only the canonical primary command changes.
  primary.params.config.mcp_servers.assay.command = "/wrong/canonical/assay-mcp-server";

  const hidden = classifyRecord(bound);
  assert.equal(
    allIntendedCellsPass(hidden),
    false,
    "canonical topology must reject a wrong primary command",
  );
  assert.notEqual(
    hidden.externalAttestation,
    "pass",
    "wrong canonical primary command must not invent attestation while decoy rows stay valid",
  );
  assert.match(
    VALIDATOR_SRC,
    /verifyLiveIdentity|observedIdentityBound/,
  );
  assert.doesNotMatch(
    VALIDATOR_SRC,
    /events\.find\(\s*\(?\s*event\s*\)?\s*=>\s*event\.method\s*===\s*"thread\/start"/,
    "live identity must not rescan raw events for thread/start",
  );

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(
    classifyRecord(liveBoundRecord(control.manifest, control.events)).externalAttestation,
    "not_provided",
    "no-op live-bound control still cannot pass on fake events",
  );
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

function retargetPairId(events, method, fromId, toId) {
  for (const event of events) {
    if (event.method === method && event.id === fromId) {
      event.id = toId;
    }
  }
}

function assertHostileProof(manifest, events, label) {
  const hostile = classifyRecord({ ...manifest, events });
  assert.equal(allIntendedCellsPass(hostile), false, `${label} must not yield 9/9`);
  const hostileRoot = scratch();
  fs.mkdirSync(hostileRoot, { recursive: true });
  rewriteProof(hostileRoot, manifest, events, hostile);
  assert.equal(validateProofRoot(hostileRoot).ok, false, `${label} must not validate as proof`);
}

test("retained-proof RPC IDs reject boolean/unsafe/fractional/null/object and sequential reuse; numeric/string controls stay green", async () => {
  // Retained-proof contract only: IDs already recorded in this #2684 pack.
  // Not a general JSON-RPC uniqueness or compatibility rule.
  const { events, manifest, classified, proofRoot } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  assert.equal(validateProofRoot(proofRoot).ok, true);
  const initialize = events.find(
    (event) => event.direction === "client" && event.method === "initialize",
  );
  assert.equal(typeof initialize.id, "number");
  assert.equal(Number.isSafeInteger(initialize.id), true);
  const elicit = events.find(
    (event) =>
      event.direction === "server" && event.method === "mcpServer/elicitation/request",
  );
  assert.equal(typeof elicit.id, "string");
  assert.ok(elicit.id.length > 0);

  const forgedObject = { forged: true };
  const cases = [
    { label: "boolean id", id: true },
    { label: "unsafe integer id", id: Number.MAX_SAFE_INTEGER + 1 },
    { label: "fractional id", id: 1.5 },
    { label: "null id", id: null },
    { label: "object id", id: forgedObject },
  ];
  for (const { label, id } of cases) {
    const mutated = structuredClone(events);
    retargetPairId(mutated, "initialize", 1, id);
    assertHostileProof(manifest, mutated, label);
  }

  const reused = structuredClone(events);
  retargetPairId(reused, "skills/list", 2, 1);
  assertHostileProof(manifest, reused, "sequential reuse of resolved id 1");

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("typed retained-method rows reject null/scalar params, unknown item type, and malformed/orphaned/duplicate elicitation; legitimate variants stay green", async () => {
  const { events, manifest, classified, proofRoot } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  assert.equal(validateProofRoot(proofRoot).ok, true);
  const item = events.find(
    (event) => event.direction === "server" && event.method === "item/completed",
  );
  const turn = events.find(
    (event) => event.direction === "server" && event.method === "turn/completed",
  );
  const elicit = events.find(
    (event) =>
      event.direction === "server" && event.method === "mcpServer/elicitation/request",
  );
  assert.ok(item && turn && elicit, "valid pack must retain the typed method rows");

  const hostiles = [
    {
      label: "item/completed params=null",
      row: { direction: "server", method: "item/completed", id: null, params: null },
    },
    {
      label: "turn/completed scalar params",
      row: { direction: "server", method: "turn/completed", id: null, params: "scalar" },
    },
    {
      label: "unknown item type",
      row: {
        direction: "server",
        method: "item/completed",
        id: null,
        params: {
          ...structuredClone(item.params),
          item: { type: "not-a-retained-item", id: "extra-1" },
        },
      },
    },
    {
      label: "malformed elicitation",
      row: {
        direction: "server",
        method: "mcpServer/elicitation/request",
        id: "elicit-malformed",
        params: { serverName: "assay" },
      },
    },
    {
      label: "orphaned elicitation",
      row: {
        direction: "server",
        method: "mcpServer/elicitation/request",
        id: "elicit-orphan",
        params: {
          serverName: "other",
          threadId: "other-thread",
          turnId: "other-turn",
          message: "not the decide probe",
          mode: "form",
          requestedSchema: { type: "object", properties: {} },
        },
      },
    },
    {
      label: "duplicate elicitation id",
      row: {
        ...structuredClone(elicit),
        params: {
          ...structuredClone(elicit.params),
          message: "duplicate id, not the decide probe",
        },
      },
    },
  ];
  for (const { label, row } of hostiles) {
    const mutated = structuredClone(events);
    mutated.push(row);
    assertHostileProof(manifest, mutated, label);
  }

  const interleaved = await drive("early-user-then-tool");
  assert.equal(interleaved.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(validateProofRoot(interleaved.proofRoot).ok, true);
  const itemTypes = interleaved.events
    .filter((event) => event.method === "item/completed")
    .map((event) => event.params?.item?.type);
  assert.deepEqual(itemTypes, ["userMessage", "mcpToolCall"]);

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("explicit invalid RPC id on a retained notification is not the canonical no-id form; control stays green", async () => {
  // Retained-proof ID contract only. Not a general JSON-RPC uniqueness rule.
  const { events, manifest, classified, proofRoot } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  assert.equal(validateProofRoot(proofRoot).ok, true);

  const item = events.find(
    (event) => event.direction === "server" && event.method === "item/completed",
  );
  assert.ok(item, "valid pack must retain item/completed");
  assert.equal(
    Object.prototype.hasOwnProperty.call(item, "id"),
    true,
    "recorder stores item/completed with an explicit id field",
  );
  assert.equal(item.id, null, "canonical retained item/completed id is null");

  const initialized = events.find(
    (event) => event.direction === "client" && event.method === "initialized",
  );
  assert.ok(initialized, "valid pack must retain initialized");
  assert.equal(
    Object.prototype.hasOwnProperty.call(initialized, "id"),
    false,
    "recorder stores initialized without an id field",
  );

  const forgedItem = structuredClone(events);
  const forgedItemRow = forgedItem.find(
    (event) => event.direction === "server" && event.method === "item/completed",
  );
  forgedItemRow.id = true;
  assertHostileProof(
    manifest,
    forgedItem,
    "explicit boolean id on item/completed must not equal absent id",
  );

  const forgedInit = structuredClone(events);
  const forgedInitRow = forgedInit.find(
    (event) => event.direction === "client" && event.method === "initialized",
  );
  forgedInitRow.id = true;
  assertHostileProof(
    manifest,
    forgedInit,
    "explicit boolean id on initialized must not equal absent id",
  );

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("retained driver/error contradicts topology; all-pass and proof stay false; control stays green", async () => {
  const { events, manifest, classified, proofRoot } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  assert.equal(validateProofRoot(proofRoot).ok, true);

  const mutated = structuredClone(events);
  mutated.push({
    direction: "driver",
    method: "error",
    params: { message: "recorded failure" },
  });
  const hostile = classifyRecord({ ...manifest, events: mutated });
  assert.equal(allIntendedCellsPass(hostile), false, "driver error must contradict 9/9");
  assert.equal(hostile.cells.driverCompleted.status, "fail");
  const hostileRoot = scratch();
  fs.mkdirSync(hostileRoot, { recursive: true });
  rewriteProof(hostileRoot, manifest, mutated, hostile);
  assert.equal(validateProofRoot(hostileRoot).ok, false);

  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true);
  assert.equal(validateProofRoot(control.proofRoot).ok, true);
});

test("review false-green: lifecycle rows and contradictory result projections", async () => {
  const { events, manifest, classified, proofRoot } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);
  assert.equal(validateProofRoot(proofRoot).ok, true);

  const candidates = [];
  candidates.push({ label: "missing initialized", events: events.filter((event) => !(event.direction === "client" && event.method === "initialized")), proofValid: false });

  const invalidInitialized = structuredClone(events);
  const invalidInitializedRow = invalidInitialized.find(
    (event) => event.direction === "client" && event.method === "initialized",
  );
  assert.ok(invalidInitializedRow);
  invalidInitializedRow.params = { unexpected: true };
  candidates.push({
    label: "invalid initialized params",
    events: invalidInitialized,
    proofValid: false,
  });

  const duplicateInitialized = structuredClone(events);
  const initialized = duplicateInitialized.find((event) => event.direction === "client" && event.method === "initialized");
  assert.ok(initialized);
  duplicateInitialized.push(structuredClone(initialized));
  candidates.push({ label: "duplicate initialized", events: duplicateInitialized, proofValid: false });

  const unrelatedFailed = structuredClone(events);
  const terminal = unrelatedFailed.find((event) => event.direction === "server" && event.method === "turn/completed");
  assert.ok(terminal);
  const extraTerminal = structuredClone(terminal);
  extraTerminal.params.threadId = "unrelated-thread";
  if (extraTerminal.params.turn && typeof extraTerminal.params.turn === "object") {
    extraTerminal.params.turn.id = "unrelated-turn";
    extraTerminal.params.turn.status = "failed";
  }
  extraTerminal.params.status = "failed";
  unrelatedFailed.push(extraTerminal);
  candidates.push({ label: "unrelated failed terminal", events: unrelatedFailed, proofValid: false });

  const contradictoryResult = structuredClone(events);
  const call = toolCompleted(contradictoryResult);
  assert.ok(call?.params?.item?.result);
  call.params.item.result.structuredContent = {
    allowed: true,
    reason: "Allowed by policy",
  };
  call.params.item.result.content = [
    { type: "text", text: "not-json" },
    { type: "image", data: "unsupported" },
  ];
  syncTerminalToolItem(contradictoryResult);
  candidates.push({
    label: "structuredContent masks contradictory content",
    events: contradictoryResult,
    proofValid: false,
    structuredResultStatus: "fail",
  });

  const unsupportedBlock = structuredClone(events);
  const unsupportedCall = toolCompleted(unsupportedBlock);
  unsupportedCall.params.item.result.content.push({ type: "image", data: "unsupported" });
  syncTerminalToolItem(unsupportedBlock);
  candidates.push({
    label: "valid text plus unsupported content block",
    events: unsupportedBlock,
    proofValid: false,
    structuredResultStatus: "fail",
  });

  const scalarStructured = structuredClone(events);
  const scalarStructuredCall = toolCompleted(scalarStructured);
  scalarStructuredCall.params.item.result.structuredContent = "scalar";
  syncTerminalToolItem(scalarStructured);
  candidates.push({
    label: "valid text masks scalar structuredContent",
    events: scalarStructured,
    proofValid: false,
    structuredResultStatus: "fail",
  });

  const disagreeingProjections = structuredClone(events);
  const disagreeingCall = toolCompleted(disagreeingProjections);
  disagreeingCall.params.item.result.structuredContent = {
    allowed: true,
    reason: "Allowed by policy",
  };
  disagreeingCall.params.item.result.content = [
    {
      type: "text",
      text: JSON.stringify({ allowed: false, reason: "Different projection" }),
    },
  ];
  syncTerminalToolItem(disagreeingProjections);
  candidates.push({
    label: "valid projections disagree",
    events: disagreeingProjections,
    proofValid: false,
    structuredResultStatus: "fail",
  });

  const observed = [];
  for (const candidate of candidates) {
    const hostile = classifyRecord({ ...manifest, events: candidate.events });
    const root = scratch();
    fs.mkdirSync(root, { recursive: true });
    rewriteProof(root, manifest, candidate.events, hostile);
    observed.push({
      label: candidate.label,
      allPass: allIntendedCellsPass(hostile),
      proofValid: validateProofRoot(root).ok,
      expectedProofValid: candidate.proofValid,
      structuredResultStatus: hostile.cells.structuredResultValidated.status,
      expectedStructuredResultStatus: candidate.structuredResultStatus,
    });
  }
  console.error(`REVIEW_OBSERVED ${JSON.stringify(observed)}`);
  assert.equal(
    observed.every(
      (row) =>
        row.allPass === false &&
        row.proofValid === row.expectedProofValid &&
        (row.expectedStructuredResultStatus == null ||
          row.structuredResultStatus === row.expectedStructuredResultStatus),
    ),
    true,
    "every hostile lifecycle/result row must match its bounded cell and proof expectations",
  );
});

test("review closeout: terminal item must agree with item/completed", async () => {
  const { events, manifest, classified } = await drive("valid");
  assert.equal(allIntendedCellsPass(classified), true);

  const contradictoryTerminal = structuredClone(events);
  const completedCall = toolCompleted(contradictoryTerminal);
  const terminal = contradictoryTerminal.find(
    (event) => event.direction === "server" && event.method === "turn/completed",
  );
  assert.ok(completedCall?.params?.item);
  assert.ok(terminal?.params?.turn);
  const failedTerminalItem = structuredClone(completedCall.params.item);
  failedTerminalItem.status = "failed";
  failedTerminalItem.result.isError = true;
  terminal.params.turn.items = [failedTerminalItem];
  const contradicted = classifyRecord({ ...manifest, events: contradictoryTerminal });
  assert.notEqual(
    contradicted.cells.oneToolInvoked.status,
    "pass",
    "terminal tool item must agree with the canonical item/completed record",
  );

});

test("review closeout: present isError must be Boolean", async () => {
  const { events, manifest } = await drive("valid");
  const malformedIsError = structuredClone(events);
  toolCompleted(malformedIsError).params.item.result.isError = "true";
  syncTerminalToolItem(malformedIsError);
  const malformed = classifyRecord({ ...manifest, events: malformedIsError });
  assert.equal(
    malformed.cells.structuredResultValidated.status,
    "fail",
    "present isError must be Boolean before its value is interpreted",
  );

});

test("review closeout: retained host rows use a credential-free closed projection", async () => {
  const credential = await drive("credential-shaped-description");
  const retained = fs.readFileSync(path.join(credential.proofRoot, "events.json"), "utf8");
  assert.doesNotMatch(
    retained,
    /ghp_0123456789abcdefghijklmnopqrstuvwxyz/,
    "free-form host descriptions must not enter the retained closed projection",
  );
  assert.equal(allIntendedCellsPass(credential.classified), true);

});

test("review closeout: retained initialized is the exact wire notification", async () => {
  const strictWire = await drive("strict-initialized-wire");
  assert.equal(
    allIntendedCellsPass(strictWire.classified),
    true,
    "the retained initialized notification must be the exact object written on the wire",
  );
});

test("review closeout: a retained record cannot self-attest external origin", async () => {
  const control = await drive("valid");
  const relabeled = liveBoundRecord(control.manifest, control.events);
  const initialize = relabeled.events.find(
    (event) => event.direction === "server" && event.method === "initialize",
  );
  assert.ok(initialize?.result);
  initialize.result.userAgent = "codex_cli/observed-host";

  const classified = classifyRecord(relabeled);
  assert.equal(
    Object.prototype.hasOwnProperty.call(classified, "liveAcceptance"),
    false,
    "self-authored bytes must not expose a positive live-proof surface",
  );
  assert.equal(classified.externalAttestation, "not_provided");
});

test("review closeout: host subjects are proof-owned and independently revalidatable", () => {
  const before = new Set(snapRoots());
  const proofRoot = portableLiveProofRoot();
  try {
    const observed = driveCli("valid", "tool", {
      captureMode: "synthetic-fixture",
      proofRoot,
    });
    assert.equal(fs.existsSync(path.join(proofRoot, "manifest.json")), true);
    const manifest = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
    const leaked = snapRoots().filter((name) => !before.has(name));
    assert.deepEqual(leaked, [], "successful capture must not leave an unowned temp snapshot");
    for (const role of ["codex", "assayMcp"]) {
      const relative = path.relative(proofRoot, manifest.hostIdentity[role].path);
      assert.equal(relative.startsWith("..") || path.isAbsolute(relative), false);
      assert.equal(fs.existsSync(manifest.hostIdentity[role].path), true);
    }
    fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
    fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    const events = JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8"));
    const topology = consumeJourneyTopology(events, manifest.journey);
    assert.equal(sha256File(manifest.hostIdentity.codex.path), manifest.hostIdentity.codex.sha256);
    assert.equal(sha256File(manifest.hostIdentity.assayMcp.path), manifest.hostIdentity.assayMcp.sha256);
    assert.equal(
      topology.primaryThread.request.params.config.mcp_servers.assay.command,
      manifest.hostIdentity.assayMcp.path,
    );
    assert.equal(manifest.invocation.argv[0], manifest.hostIdentity.codex.path);
    assert.equal(
      verifyLiveIdentity(
        manifest.hostIdentity,
        manifest.invocation,
        topology,
        proofRoot,
        manifest.journey,
      ),
      true,
      JSON.stringify({ hostIdentity: manifest.hostIdentity, invocation: manifest.invocation }),
    );
    const checked = validateProofRoot(proofRoot);
    assert.equal(checked.ok, true, checked.reasons.join("; "));
  } finally {
    for (const name of snapRoots()) {
      if (!before.has(name)) {
        fs.rmSync(path.join(os.tmpdir(), name), { recursive: true, force: true });
      }
    }
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("verifyLiveIdentity requires an explicit private proof root", {
  skip: process.platform === "win32",
}, () => {
  const proofRoot = portableLiveProofRoot();
  let observed;
  try {
    observed = driveCli("valid", "tool", {
      captureMode: "synthetic-fixture",
      proofRoot,
    });
    const manifest = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
    const events = JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8"));
    const topology = consumeJourneyTopology(events, manifest.journey);
    const verify = (root) =>
      verifyLiveIdentity(
        manifest.hostIdentity,
        manifest.invocation,
        topology,
        root,
        manifest.journey,
      );

    assert.equal(verify(proofRoot), true, "private explicit root is the positive control");
    assert.equal(
      verifyLiveIdentity(manifest.hostIdentity, manifest.invocation, topology),
      false,
      "an omitted root must not bypass subject containment",
    );
    assert.equal(verify(null), false, "a null root must not bypass subject containment");
    fs.chmodSync(proofRoot, 0o755);
    assert.equal(verify(proofRoot), false, "a public root must not bind live identity");
    fs.chmodSync(proofRoot, 0o500);
    assert.equal(verify(proofRoot), false, "an owner-only non-0700 root must not bind live identity");
  } finally {
    fs.chmodSync(proofRoot, 0o700);
    if (observed) {
      fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
      fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    }
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("review closeout: proof-owned subject mutations fail closed", () => {
  const proofRoot = portableLiveProofRoot();
  let observed;
  try {
    observed = driveCli("valid", "tool", {
      captureMode: "synthetic-fixture",
      proofRoot,
    });
    const manifest = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
    const codex = manifest.hostIdentity.codex.path;
    const assayMcp = manifest.hostIdentity.assayMcp.path;
    const original = fs.readFileSync(codex);
    const originalMode = fs.statSync(codex).mode & 0o777;
    const restore = () => {
      fs.rmSync(codex, { force: true });
      fs.writeFileSync(codex, original);
      fs.chmodSync(codex, originalMode);
    };
    const rejects = (label) => {
      const checked = validateProofRoot(proofRoot);
      assert.equal(checked.ok, false, `${label} must invalidate the pack`);
    };

    assert.equal(validateProofRoot(proofRoot).ok, true, "unchanged control must validate");

    fs.rmSync(codex);
    rejects("missing subject");
    restore();

    fs.copyFileSync(assayMcp, codex);
    rejects("replaced subject");
    restore();

    fs.appendFileSync(codex, "\nmutated\n");
    rejects("altered subject");
    restore();

    if (process.platform !== "win32") {
      fs.rmSync(codex);
      fs.symlinkSync(assayMcp, codex);
      rejects("symlink subject");
      restore();

      fs.chmodSync(codex, 0o600);
      rejects("non-executable subject");
      restore();
    }

    assert.equal(validateProofRoot(proofRoot).ok, true, "restored no-op control must validate");
  } finally {
    if (observed) {
      fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
      fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    }
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("review closeout: identity acquisition does not manufacture executability", () => {
  const before = new Set(snapRoots());
  const codex = writeVersionOnlyBin("codex", "codex-nonexec/0.0.0");
  const mcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-nonexec/0.0.0");
  fs.chmodSync(codex, 0o600);
  fs.chmodSync(mcp, 0o600);
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(codex)}${path.delimiter}${path.dirname(mcp)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(() => resolveHostIdentity(), /executable|execute access/i);
  } finally {
    process.env.PATH = previousPath;
    for (const name of snapRoots()) {
      if (!before.has(name)) {
        fs.rmSync(path.join(os.tmpdir(), name), { recursive: true, force: true });
      }
    }
  }
});

test("independent review closeout: manifest cannot add attestation or mutable identity claims", () => {
  const proofRoot = portableLiveProofRoot();
  let observed;
  try {
    observed = driveCli("valid", "tool", {
      captureMode: "synthetic-fixture",
      proofRoot,
    });
    assert.equal(validateProofRoot(proofRoot).ok, true, "unaltered control must validate");
    const manifestPath = path.join(proofRoot, "manifest.json");
    const control = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
    for (const [label, mutate] of [
      ["self-attested top-level claims", (manifest) => {
        manifest.externalAttestation = "verified";
        manifest.liveAcceptance = { status: "pass" };
        manifest.authenticatedOrigin = { provider: "self-authored" };
      }],
      ["mutable identity metadata", (manifest) => {
        manifest.hostIdentity.os = "forged-os";
        manifest.hostIdentity.arch = "forged-arch";
        manifest.hostIdentity.codex.version = "forged-version";
        manifest.hostIdentity.codex.installSource = "forged-source";
      }],
    ]) {
      const manifest = structuredClone(control);
      mutate(manifest);
      fs.writeFileSync(manifestPath, stableStringify(manifest));
      const checked = validateProofRoot(proofRoot);
      assert.equal(checked.ok, false, `${label} must fail closed`);
    }
  } finally {
    if (observed) {
      fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
      fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    }
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("independent review closeout: terminal must contain the one canonical tool item", async () => {
  const { events, manifest } = await drive("valid");
  const completed = toolCompleted(events).params.item;
  for (const [label, items] of [
    ["omitted", []],
    ["different id", [{ ...structuredClone(completed), id: "different-call" }]],
  ]) {
    const mutated = structuredClone(events);
    const terminal = mutated.find(
      (event) => event.direction === "server" && event.method === "turn/completed",
    );
    assert.ok(terminal?.params?.turn);
    terminal.params.turn.items = items;
    const classified = classifyRecord({ ...manifest, events: mutated });
    assert.notEqual(
      classified.cells.oneToolInvoked.status,
      "pass",
      `${label} terminal tool item must not pass`,
    );
  }
});

test("independent review closeout: client request params are a closed projection", async () => {
  const { events, manifest, proofRoot } = await drive("valid");
  const mutated = structuredClone(events);
  const start = mutated.find(
    (event) => event.direction === "client" && event.method === "thread/start",
  );
  assert.ok(start?.params?.config?.mcp_servers?.assay);
  start.params.config.mcp_servers.assay.description =
    "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
  const eventsText = stableStringify(mutated);
  const rewrittenManifest = {
    ...manifest,
    hashes: { events: sha256Utf8(eventsText) },
  };
  const classified = classifyRecord({ ...rewrittenManifest, events: mutated });
  fs.writeFileSync(path.join(proofRoot, "events.json"), eventsText);
  fs.writeFileSync(path.join(proofRoot, "manifest.json"), stableStringify(rewrittenManifest));
  fs.writeFileSync(path.join(proofRoot, "classification.json"), stableStringify(classified));
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, false, "unexpected nested client fields must fail closed");
});

test("review repair: host-observation cannot omit proof-owned identity subjects", () => {
  const proofRoot = portableLiveProofRoot();
  let observed;
  try {
    observed = driveCli("valid", "tool", {
      captureMode: "synthetic-fixture",
      proofRoot,
    });
    assert.equal(validateProofRoot(proofRoot).ok, true, "identity-bound control must validate");

    const manifest = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
    const events = JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8"));
    const initialize = events.find(
      (event) => event.direction === "server" && event.method === "initialize",
    );
    assert.ok(initialize?.result);
    initialize.result.userAgent = "[observed-host]";
    manifest.captureMode = "host-observation";
    manifest.initialize.userAgent = "[observed-host]";
    manifest.hashes = { events: sha256Utf8(stableStringify(events)) };
    const hostClassified = classifyRecord({ ...manifest, events });
    rewriteProof(proofRoot, manifest, events, hostClassified);
    const hostControl = validateProofRoot(proofRoot);
    assert.equal(hostControl.ok, true, hostControl.reasons.join("; "));

    manifest.hostIdentity = null;
    manifest.allowlist = ["classification.json", "events.json", "manifest.json"];
    for (const subject of ["codex.snapshot", "assay-mcp-server.snapshot"]) {
      fs.rmSync(path.join(proofRoot, subject));
    }
    const identityFree = classifyRecord({ ...manifest, events });
    rewriteProof(proofRoot, manifest, events, identityFree);
    assert.equal(
      validateProofRoot(proofRoot).ok,
      false,
      "host-observation without identity subjects must fail closed",
    );
  } finally {
    if (observed) {
      fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
      fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    }
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("review repair: nested MCP argv is credential-free", async () => {
  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true, "unchanged control must pass");
  const marker = "nested_probe_secret_2737";
  const primary = control.events.find(
    (event) =>
      event.direction === "client" &&
      event.method === "thread/start" &&
      !String(event.params?.config?.mcp_servers?.assay?.command ?? "").includes(
        "missing-assay-mcp-server",
      ) &&
      !event.params?.config?.mcp_servers?.assay?.args?.some((arg) =>
        String(arg).includes("missing-policy-root"),
      ),
  );
  assert.ok(primary?.params?.config?.mcp_servers?.assay);

  const credentialParams = structuredClone(primary.params);
  credentialParams.config.mcp_servers.assay.args = ["--token", marker];
  const projected = projectClientRequestParams("thread/start", credentialParams);
  assert.doesNotMatch(
    JSON.stringify(projected),
    new RegExp(marker),
    "credential values must not enter the retained projection",
  );
  const credentialEvents = structuredClone(control.events);
  credentialEvents[control.events.indexOf(primary)].params = credentialParams;
  assert.equal(
    allIntendedCellsPass(classifyRecord({ ...control.manifest, events: credentialEvents })),
    false,
    "credential-bearing primary argv must not classify clean",
  );

});

test("review repair: each MCP thread role has one canonical command and argv", async () => {
  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true, "unchanged control must pass");
  const extraArgEvents = structuredClone(control.events);
  const primary = extraArgEvents.find(
    (event) =>
      event.direction === "client" &&
      event.method === "thread/start" &&
      !String(event.params?.config?.mcp_servers?.assay?.command ?? "").includes(
        "missing-assay-mcp-server",
      ) &&
      !event.params?.config?.mcp_servers?.assay?.args?.some((arg) =>
        String(arg).includes("missing-policy-root"),
      ),
  );
  assert.ok(primary?.params?.config?.mcp_servers?.assay?.args);
  primary.params.config.mcp_servers.assay.args.push("--verbose");
  assert.equal(
    allIntendedCellsPass(classifyRecord({ ...control.manifest, events: extraArgEvents })),
    false,
    "a noncanonical primary role argv must not classify clean",
  );
});

test("review repair: host manifest invocation is exact and credential-free", () => {
  const proofRoot = portableLiveProofRoot();
  let observed;
  try {
    observed = driveCli("valid", "tool", {
      captureMode: "synthetic-fixture",
      proofRoot,
    });
    assert.equal(validateProofRoot(proofRoot).ok, true, "unchanged control must validate");
    const manifest = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
    const events = JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8"));
    for (const [label, mutate] of [
      ["argv addition", (candidate) => {
        candidate.invocation.argv.push("--token", "sensitive-test-value");
      }],
      ["environment-name addition", (candidate) => {
        candidate.invocation.envNames.push("ANTHROPIC_API_KEY");
      }],
    ]) {
      const candidate = structuredClone(manifest);
      mutate(candidate);
      const classified = classifyRecord({ ...candidate, events });
      rewriteProof(proofRoot, candidate, events, classified);
      assert.equal(validateProofRoot(proofRoot).ok, false, `${label} must fail closed`);
    }
  } finally {
    if (observed) {
      fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
      fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    }
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});
