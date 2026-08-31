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
  classifyRecord,
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
  assert.notEqual(classified.cells.oneToolInvoked.status, "pass");
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
