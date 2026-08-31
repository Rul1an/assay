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
import { runProof } from "./codex_host_proof.mjs";
import {
  CELLS,
  DECIDE_INPUT,
  DECIDE_TOOL,
  classifyRecord,
  decidePrompt,
  validateProofRoot,
} from "./codex_host_proof_validator.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FAKE = path.join(HERE, "fixtures/codex-host-proof/fake-app-server.mjs");
const DRIVER_SRC = fs.readFileSync(path.join(HERE, "codex_host_proof.mjs"), "utf8");
const VALIDATOR_SRC = fs.readFileSync(path.join(HERE, "codex_host_proof_validator.mjs"), "utf8");

function scratch() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "assay-2684-"));
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
  const result = await runProof({
    provenance: "synthetic",
    timeoutMs: 4000,
    maxBytes: 1_048_576,
    journey,
    allowLiveTurn: false,
    childArgv: ["node", FAKE, "--scenario", scenario, "--project-root", projectRoot],
    proofRoot,
    projectRoot,
    mcpCommand: path.join(projectRoot, "install/bin/assay-mcp-server"),
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

function driveCli(scenario, journey = "tool") {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const result = spawnSync(
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
      JSON.stringify(["node", FAKE, "--scenario", scenario, "--project-root", projectRoot]),
      "--mcp-command",
      path.join(projectRoot, "install/bin/assay-mcp-server"),
      "--journey",
      journey,
      "--timeout-ms",
      "4000",
    ],
    { encoding: "utf8", timeout: 15_000 },
  );
  return { ...result, proofRoot, projectRoot };
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
  const { classified, manifest, proofRoot, driverExitCode, events } = await drive("valid");
  assert.equal(manifest.provenance, "synthetic");
  assert.equal(driverExitCode, 0);
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
  const { classified, manifest, proofRoot, driverExitCode } = await drive(
    "exit-1-after-success",
  );
  assert.notEqual(driverExitCode, 0);
  assert.equal(classified.cells.oneToolInvoked.status, "pass");
  assert.equal(manifest.driverExitCode, driverExitCode);
  assert.notEqual(classified.liveAcceptance.status, "pass");
  const checked = validateProofRoot(proofRoot);
  assert.equal(checked.ok, true);
  assert.notEqual(checked.classified.liveAcceptance.status, "pass");
  const relabeled = classifyRecord({
    ...manifest,
    events: JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8")),
    driverExitCode: 0,
    provenance: "live",
  });
  assert.notEqual(
    relabeled.liveAcceptance.status,
    "pass",
    "rewriting exit 0 on synthetic fake events must not mint live proof",
  );
  const cli = driveCli("exit-1-after-success");
  assert.notEqual(cli.status, 0, "driver CLI must not exit 0 when the child exits 1");
  assert.match(cli.stdout, /"driverExitCode":1/);
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
  const projectRoot = seedProject();
  const proofRoot = scratch();
  return runProof({
    provenance: "synthetic",
    timeoutMs: extra.timeoutMs ?? 4000,
    maxBytes: extra.maxBytes ?? 1_048_576,
    journey: extra.journey ?? "tool",
    allowLiveTurn: false,
    childArgv,
    proofRoot,
    projectRoot,
    mcpCommand: path.join(projectRoot, "install/bin/assay-mcp-server"),
    ...extra,
    childArgv,
    proofRoot,
    projectRoot,
  }).then((result) => ({ ...result, proofRoot, projectRoot }));
}

function driveCliInline(childArgv, extra = {}) {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const args = [
    path.join(HERE, "codex_host_proof.mjs"),
    "--provenance",
    "synthetic",
    "--proof-root",
    proofRoot,
    "--project-root",
    projectRoot,
    "--child-argv",
    JSON.stringify(childArgv),
    "--mcp-command",
    path.join(projectRoot, "install/bin/assay-mcp-server"),
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
    mcpCommand: path.join(projectRoot, "install/bin/assay-mcp-server"),
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
    mcpCommand: path.join(projectRoot, "install/bin/assay-mcp-server"),
    timeoutMs: 4000,
    maxBytes: 1_048_576,
  };
  await assert.rejects(() => runProof({ ...base, maxBytes: 0 }), /finite positive/);
  await assert.rejects(() => runProof({ ...base, timeoutMs: Number.POSITIVE_INFINITY }), /finite positive/);
  const control = await drive("valid");
  assert.equal(control.classified.cells.driverCompleted.status, "pass");
});
