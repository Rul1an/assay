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
import { parseArgs, resolveHostIdentity, runProof } from "./codex_host_proof.mjs";
import {
  CELLS,
  DECIDE_INPUT,
  DECIDE_TOOL,
  classifyRecord,
  decidePrompt,
  forbiddenProofRoot,
  sha256Utf8,
  stableStringify,
  validateProofRoot,
} from "./codex_host_proof_validator.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FAKE = path.join(HERE, "fixtures/codex-host-proof/fake-app-server.mjs");
const DRIVER_SRC = fs.readFileSync(path.join(HERE, "codex_host_proof.mjs"), "utf8");
const VALIDATOR_SRC = fs.readFileSync(path.join(HERE, "codex_host_proof_validator.mjs"), "utf8");

function scratch() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "assay-2684-"));
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
    provenance: "synthetic",
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

function writeShadowCodex(childArgv) {
  const binDir = scratch();
  const bin = path.join(binDir, "codex");
  const script = `#!/usr/bin/env node
import { spawn } from "node:child_process";
if (process.argv.includes("--version")) {
  process.stdout.write("codex-shadow/0.0.0\\n");
  process.exit(0);
}
const child = spawn(${JSON.stringify(childArgv[0])}, ${JSON.stringify(childArgv.slice(1))}, { stdio: "inherit" });
child.on("close", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
`;
  fs.writeFileSync(bin, script, { mode: 0o755 });
  return bin;
}

function writeShadowMcp() {
  const bin = path.join(scratch(), "assay-mcp-server");
  fs.writeFileSync(
    bin,
    `#!/usr/bin/env node
if (process.argv.includes("--version")) {
  process.stdout.write("assay-mcp-server-shadow/0.0.0\\n");
  process.exit(0);
}
process.stdout.write("assay-mcp-server-shadow/0.0.0\\n");
`,
    { mode: 0o755 },
  );
  return bin;
}

function driveCli(scenario, journey = "tool", extra = {}) {
  const projectRoot = seedProject();
  const proofRoot = extra.proofRoot ?? scratch();
  const mcpBin = extra.assayMcpBin ?? writeShadowMcp();
  const codexBin =
    extra.codexBin ??
    writeShadowCodex(["node", FAKE, "--scenario", scenario, "--project-root", projectRoot]);
  const args = [
    path.join(HERE, "codex_host_proof.mjs"),
    "--provenance",
    extra.provenance ?? "synthetic",
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

test("synthetic positive control: every intended cell passes; liveAcceptance must not", async () => {
  const { classified, manifest, proofRoot, driverOutcome, childExitCode, events } = await drive("valid");
  assert.equal(manifest.provenance, "synthetic");
  assert.equal(manifest.schema, "assay.codex-host-proof.v2");
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
  assert.notEqual(classified.liveAcceptance.status, "pass");
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
  assert.notEqual(checked.classified.liveAcceptance.status, "pass");
});

test("no-op discovery control does not invent tool or MCP passes", async () => {
  const { classified } = await drive("valid", "discovery");
  assert.notEqual(classified.cells.oneToolInvoked.status, "pass");
  assert.notEqual(classified.cells.mcpStarted.status, "pass");
  assert.notEqual(classified.liveAcceptance.status, "pass");
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
  assert.notEqual(classified.liveAcceptance.status, "pass");
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, true);
  assert.notEqual(checked.classified.liveAcceptance.status, "pass");
  const relabeled = classifyRecord({
    ...manifest,
    events: JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8")),
    childExitCode: 0,
    driverOutcome: { exitCode: 0, status: "pass" },
    provenance: "live",
  });
  assert.notEqual(
    relabeled.liveAcceptance.status,
    "pass",
    "rewriting exit 0 on synthetic fake events must not mint live proof",
  );
  const cli = driveCli("exit-1-after-success");
  assert.notEqual(cli.status, 0, "driver CLI must not exit 0 when the child exits 1");
  assert.match(cli.stdout, /"exitCode":1/);
});

test("synthetic events never validate as actual-host proof", async () => {
  const { manifest, proofRoot, classified } = await drive("valid");
  assert.notEqual(classified.liveAcceptance.status, "pass");
  const forged = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
  forged.provenance = "live";
  fs.writeFileSync(
    path.join(proofRoot, "manifest.json"),
    `${JSON.stringify(forged)}\n`,
  );
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, false);
  assert.match(checked.reasons.join(" "), /live provenance|fake|synthetic/i);
});

test("missing skill is not pass", async () => {
  const { classified } = await drive("missing-skill");
  assert.notEqual(classified.cells.skillDiscovered.status, "pass");
  assert.notEqual(classified.liveAcceptance.status, "pass");
});

test("wrong cwd is not pass", async () => {
  const { classified } = await drive("wrong-cwd");
  assert.notEqual(classified.cells.cwdObserved.status, "pass");
  assert.notEqual(classified.liveAcceptance.status, "pass");
});

test("missing tool is not a clean tool list", async () => {
  const { classified } = await drive("missing-tool");
  assert.notEqual(classified.cells.exactToolsListed.status, "pass");
  assert.notEqual(classified.liveAcceptance.status, "pass");
});

test("wrong tool invocation is not pass", async () => {
  const { classified } = await drive("wrong-tool");
  assert.equal(classified.cells.oneToolInvoked.status, "fail");
  assert.notEqual(classified.cells.oneToolInvoked.status, "unavailable");
  assert.notEqual(classified.liveAcceptance.status, "pass");
});

test("clean missing-binary status is not a host-failure pass", async () => {
  const { classified } = await drive("clean-missing-binary");
  assert.notEqual(classified.cells.missingBinaryNotClean.status, "pass");
  assert.notEqual(classified.liveAcceptance.status, "pass");
});

test("clean invalid-policy-root status is not a host-failure pass", async () => {
  const { classified } = await drive("clean-invalid-root");
  assert.notEqual(classified.cells.invalidPolicyRootNotClean.status, "pass");
  assert.notEqual(classified.liveAcceptance.status, "pass");
});

test("truncated stream does not pass", async () => {
  const { classified } = await drive("truncated");
  assert.notEqual(classified.liveAcceptance.status, "pass");
  assert.notEqual(classified.cells.driverCompleted.status, "pass");
});

test("unavailable stream does not pass", async () => {
  const { classified } = await drive("unavailable-stream");
  assert.notEqual(classified.liveAcceptance.status, "pass");
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

function driveInline(childArgv, extra = {}) {
  const projectRoot = extra.projectRoot ?? seedProject();
  const proofRoot = extra.proofRoot ?? scratch();
  const testOnlyChild = extra.testOnlyChild ?? spawnFakeChild(childArgv, projectRoot);
  return runProof({
    provenance: extra.provenance ?? "synthetic",
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
    "--provenance",
    "synthetic",
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
  assert.notEqual(classified.liveAcceptance.status, "pass");
  const checked = validateProofRoot(proofRoot);
  assert.notEqual(checked.classified.liveAcceptance.status, "pass");
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
    "classification.json",
    "events.json",
    "manifest.json",
  ]);
  const stored = JSON.parse(
    fs.readFileSync(path.join(cli.proofRoot, "classification.json"), "utf8"),
  );
  assert.equal(stored.cells.oneToolInvoked.status, "unavailable");
  assert.notEqual(stored.cells.oneToolInvoked.status, "pass");
  assert.notEqual(stored.liveAcceptance.status, "pass");
});

test("credential-bearing argv is rejected before spawn and not persisted", async () => {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const marker = "NONSECRET_PROBE_MARKER";
  const { classified, manifest, events } = await runProof({
    provenance: "synthetic",
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
  assert.notEqual(classified.liveAcceptance.status, "pass");
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
  assert.notEqual(delayed.classified.liveAcceptance.status, "pass");
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
  assert.notEqual(failed.classified.liveAcceptance.status, "pass");
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
  assert.doesNotMatch(VALIDATOR_SRC, /\.isError\b/);
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
    provenance: "synthetic",
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

test("forged live provenance and initialize cannot mint a live pass from synthetic events", async () => {
  const { manifest, proofRoot, events, classified } = await drive("valid");
  assert.notEqual(classified.liveAcceptance.status, "pass");
  const forged = {
    ...JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8")),
    provenance: "live",
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
    relabeled.liveAcceptance.status,
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
  assert.notEqual(checked.classified?.liveAcceptance?.status, "pass");
});

test("production CLI rejects --child-argv; credential name variants are rejected before spawn", async () => {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const cli = spawnSync(
    process.execPath,
    [
      path.join(HERE, "codex_host_proof.mjs"),
      "--provenance",
      "synthetic",
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
      provenance: "synthetic",
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
    provenance: "synthetic",
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
  assert.notEqual(flood.classified.liveAcceptance.status, "pass");
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
    const reason = forbiddenProofRoot(proofRoot, "synthetic");
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
  const reason = forbiddenProofRoot(root, "live");
  if (reason) {
    fs.rmSync(root, { recursive: true, force: true });
    throw new Error(`test helper allocated a forbidden live proof root: ${reason}`);
  }
  return root;
}

test("production live identity is observed from binaries and required before live CLI exit 0", () => {
  assert.equal(
    forbiddenProofRoot(path.join("/tmp", `assay-live-reject-${process.pid}`), "live"),
    "live proof root must not be temporary storage",
  );
  const proofRoot = portableLiveProofRoot();
  try {
    const mcpBin = writeShadowMcp();
    const live = driveCli("valid", "tool", {
      provenance: "live",
      allowLiveTurn: true,
      assayMcpBin: mcpBin,
      proofRoot,
    });
    const manifestPath = path.join(live.proofRoot, "manifest.json");
    assert.equal(fs.existsSync(manifestPath), true, "live CLI must still write a pack");
    const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
    const identity = manifest.hostIdentity;
    assert.ok(identity && typeof identity === "object", "production CLI must construct hostIdentity");
    assert.equal(typeof identity.os, "string");
    assert.equal(typeof identity.arch, "string");
    for (const role of ["codex", "assayMcp"]) {
      assert.equal(path.isAbsolute(identity[role].path), true, `${role} path must be absolute`);
      assert.match(identity[role].sha256, /^[a-f0-9]{64}$/);
      assert.ok(identity[role].version.length > 0, `${role} version must be observed`);
      assert.ok(identity[role].installSource.length > 0, `${role} install source must be recorded`);
      assert.equal(fs.lstatSync(identity[role].path).isFile(), true);
    }
    assert.equal(identity.assayMcp.path, fs.realpathSync(mcpBin));
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
    assert.notEqual(classified.liveAcceptance.status, "pass");
    assert.match(
      classified.liveAcceptance.reason,
      /not authenticated|not tamper-evident|no-authentication/i,
    );
    assert.notEqual(
      live.status,
      0,
      "live CLI must not exit 0 while liveAcceptance is not pass",
    );

    const forgedRoot = scratch();
    for (const name of ["manifest.json", "events.json", "classification.json"]) {
      fs.copyFileSync(path.join(live.proofRoot, name), path.join(forgedRoot, name));
    }
    const forgedEvents = JSON.parse(fs.readFileSync(path.join(forgedRoot, "events.json"), "utf8"));
    const forged = JSON.parse(fs.readFileSync(path.join(forgedRoot, "manifest.json"), "utf8"));
    forged.provenance = "live";
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
      relabeled.liveAcceptance.status,
      "pass",
      "self-attested nonexistent binary paths must not mint a live pass",
    );
    fs.writeFileSync(path.join(forgedRoot, "manifest.json"), stableStringify(forged));
    fs.writeFileSync(path.join(forgedRoot, "classification.json"), stableStringify(relabeled));
    const checked = validateProofRoot(forgedRoot);
    assert.equal(checked.ok, false);
    assert.notEqual(checked.classified?.liveAcceptance?.status, "pass");

    const control = driveCli("valid", "discovery");
    assert.ok(control.stdout.includes("synthetic") || control.status !== undefined);
  } finally {
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("production spawn ignores user childArgv and rejects --mcp-command", async () => {
  const marker = path.join(scratch(), "spawned-from-child-argv");
  const evil = path.join(scratch(), "evil-child");
  fs.writeFileSync(
    evil,
    `#!/usr/bin/env node
import fs from "node:fs";
fs.writeFileSync(${JSON.stringify(marker)}, "spawned\\n");
`,
    { mode: 0o755 },
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
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(shadow)}${path.delimiter}${previousPath}`;
  try {
    await runProof({
      provenance: "synthetic",
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
      provenance: "synthetic",
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
        provenance: "synthetic",
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
    const reason = forbiddenProofRoot(leaf, "synthetic");
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
  const control = forbiddenProofRoot(scratch(), "synthetic");
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

function productionChildCommands(src) {
  const commands = [];
  const re = /\bspawn(?:Sync)?\s*\(\s*([^,\n]+)/g;
  let match;
  while ((match = re.exec(src))) {
    commands.push(match[1].trim());
  }
  return commands;
}

function writeMarkedBin(name, marker, version) {
  const bin = path.join(scratch(), name);
  fs.writeFileSync(
    bin,
    `#!/usr/bin/env node
import fs from "node:fs";
fs.writeFileSync(${JSON.stringify(marker)}, "ran\\n");
if (process.argv.includes("--version")) {
  process.stdout.write(${JSON.stringify(version)} + "\\n");
  process.exit(0);
}
process.stdin.resume();
`,
    { mode: 0o755 },
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
        "--provenance",
        "synthetic",
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
    assert.equal(ignored.codex.path, fs.realpathSync(codexBin));
    assert.equal(ignored.assayMcp.path, fs.realpathSync(mcpBin));
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
      "--provenance",
      "synthetic",
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
  assert.equal(manifest.hostIdentity.codex.path, fs.realpathSync(codexBin));
  assert.equal(manifest.hostIdentity.assayMcp.path, fs.realpathSync(mcpBin));
  assert.equal(manifest.hostIdentity.codex.installSource, "PATH");
  assert.equal(manifest.hostIdentity.assayMcp.installSource, "PATH");
  const events = JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8"));
  const start = events.find(
    (event) => event.direction === "client" && event.method === "thread/start",
  );
  assert.equal(start.params.config.mcp_servers.assay.command, manifest.hostIdentity.assayMcp.path);
});

test("production spawn and probe use PATH names; options.codexBin is not executed", async () => {
  const flagMarker = path.join(scratch(), "flag-codex-ran");
  const pathMarker = path.join(scratch(), "path-codex-ran");
  const flagCodex = writeMarkedBin("codex", flagMarker, "codex-flag/9.9.9");
  const pathDir = scratch();
  const pathCodex = path.join(pathDir, "codex");
  fs.writeFileSync(
    pathCodex,
    `#!/usr/bin/env node
import fs from "node:fs";
import { spawn } from "node:child_process";
fs.writeFileSync(${JSON.stringify(pathMarker)}, "ran\\n");
if (process.argv.includes("--version")) {
  process.stdout.write("codex-path/0.0.0\\n");
  process.exit(0);
}
const child = spawn(${JSON.stringify(process.execPath)}, ${JSON.stringify([FAKE, "--scenario", "valid", "--project-root", "unused"])}, { stdio: "inherit" });
child.on("close", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
`,
    { mode: 0o755 },
  );
  const mcpBin = writeShadowMcp();
  const projectRoot = seedProject();
  const previousPath = process.env.PATH;
  process.env.PATH = `${pathDir}${path.delimiter}${path.dirname(mcpBin)}${path.delimiter}${previousPath}`;
  try {
    await runProof({
      provenance: "synthetic",
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

  const commands = productionChildCommands(DRIVER_SRC);
  assert.ok(commands.length >= 2, "production must probe and spawn");
  for (const command of commands) {
    assert.match(
      command,
      /^"(?:codex|assay-mcp-server)"$/,
      `production spawn/probe command must be a fixed name, got ${command}`,
    );
  }
});
