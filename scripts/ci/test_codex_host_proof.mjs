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
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  parseArgs,
  resolveHostIdentity as resolveHostIdentityProduction,
  runProof as runProofProduction,
} from "./codex_host_proof.mjs";
import {
  ALLOWED_TURN_ITEMS_VIEWS,
  CELLS,
  DECIDE_INPUT,
  DECIDE_TOOL,
  EXPECTED_TOOLS,
  HARD_MAX_SNAPSHOT_BYTES,
  HOST_SUBJECTS,
  INSTALL_ROUTES,
  PUBLISHED_INSTALL_ROUTES,
  boundedPrintable,
  installSourceBound,
  liveIdentityBound,
  projectHostIdentity,
  KNOWN_ITEM_TYPES,
  KNOWN_METHODS,
  classifyCells,
  classifyRecord,
  classifyStoredEvent,
  consumeJourneyTopology,
  decidePrompt,
  driverOutcomeFrom,
  elicitationAcceptable,
  forbiddenProofRoot,
  preSpawnFailureState,
  projectClientRequestParams,
  projectRetainedEvent,
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

test("#2823: package verification recomputes bytes and binds the declared package", async (t) => {
  const validator = await import("./codex_host_proof_validator.mjs");
  assert.equal(typeof validator.verifyAssayPackage, "function");
  const root = scratch();
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const bytes = Buffer.from("synthetic package bytes, not published evidence");
  const { createHash } = await import("node:crypto");
  const digest = createHash("sha256").update(bytes).digest("hex");
  const source = { route: "crates-io", reference: "assay-mcp-server@6.0.0" };
  const row = { name: "assay-mcp-server", vers: "6.0.0", cksum: digest, yanked: false };
  const metadata = {
    package: "assay-mcp-server 6.0.0 (registry+https://github.com/rust-lang/crates.io-index)",
    version_req: "=6.0.0", bins: ["assay-mcp-server"],
    profile: "release", target: "aarch64-apple-darwin", rustc: "rustc test fixture",
  };
  const files = {
    package: path.join(root, "package"),
    index: path.join(root, "index"),
    installation: path.join(root, "installation"),
  };
  const restore = () => {
    fs.writeFileSync(files.package, bytes);
    fs.writeFileSync(files.index, JSON.stringify(row));
    fs.writeFileSync(files.installation, JSON.stringify(metadata));
  };
  restore();
  const verify = (declared = source) => validator.verifyAssayPackage(files, declared);
  assert.equal(verify().packageSha256, digest);
  for (const [name, edit] of [
    ["changed package", () => fs.appendFileSync(files.package, "changed")],
    ["wrong checksum", () => fs.writeFileSync(files.index, JSON.stringify({ ...row, cksum: "0".repeat(64) }))],
    ["wrong crate", () => fs.writeFileSync(files.index, JSON.stringify({ ...row, name: "other" }))],
    ["wrong version", () => fs.writeFileSync(files.index, JSON.stringify({ ...row, vers: "6.0.1" }))],
    ["yanked", () => fs.writeFileSync(files.index, JSON.stringify({ ...row, yanked: true }))],
    ["wrong installation", () => fs.writeFileSync(files.installation, JSON.stringify({ ...metadata, version_req: "=6.0.1" }))],
    ["missing bytes", () => fs.unlinkSync(files.package)],
    ["oversized bytes", () => fs.writeFileSync(files.package, Buffer.alloc(4 * 1024 * 1024 + 1))],
  ]) {
    restore(); edit();
    assert.throws(verify, undefined, name);
    restore(); assert.equal(verify().packageSha256, digest, `${name} restored control`);
  }
  for (const route of ["local-build", "host-bundled", "github-release"]) {
    assert.throws(() => verify({ ...source, route }), undefined, route);
  }
  fs.writeFileSync(files.index, JSON.stringify(row, null, 2));
  assert.equal(verify().packageSha256, digest, "format-only no-op stays green");
});

test("#2823: live journeys require package evidence at the final outcome funnel", async () => {
  const { requiredCellsForJourney, driverOutcomeExit } = await import("./codex_host_proof_validator.mjs");
  for (const journey of ["discovery", "failures", "tool"]) {
    const required = requiredCellsForJourney(journey, "host-observation");
    assert.ok(required.includes("installRouteAdmissible"));
    assert.ok(required.includes("installPackageVerified"));
    const cells = Object.fromEntries([...CELLS, ...required].map((key) => [key, { status: "pass" }]));
    const pack = { childExit: 0, truncated: false, streamUnavailable: false, captureMode: "host-observation" };
    assert.equal(driverOutcomeExit(pack, cells, journey), 0);
    for (const key of ["installRouteAdmissible", "installPackageVerified"]) {
      for (const status of ["unavailable", "fail", "not_applicable"]) {
        cells[key] = { status };
        assert.notEqual(driverOutcomeExit(pack, cells, journey), 0, `${journey}/${key}/${status}`);
      }
      delete cells[key];
      assert.notEqual(driverOutcomeExit(pack, cells, journey), 0);
      cells[key] = { status: "pass" };
    }
  }
});

test("#2823: missing package prevents even the first version probe", (t) => {
  const root = scratch();
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const codexMarker = path.join(root, "codex-executed");
  const assayMarker = path.join(root, "assay-executed");
  const codex = writeMarkedBin("codex", codexMarker, "codex-fixture/0.0.0");
  const assay = writeMarkedBin("assay-mcp-server", assayMarker, "assay-mcp-server 6.0.0");
  const oldPath = process.env.PATH;
  process.env.PATH = `${path.dirname(codex)}${path.delimiter}${path.dirname(assay)}${path.delimiter}${oldPath}`;
  try {
    assert.throws(() => resolveHostIdentityProduction({
      captureMode: "host-observation", proofRoot: root,
      installSources: { ...TEST_INSTALL_SOURCES, assayMcp: { route: "crates-io", reference: "assay-mcp-server@6.0.0" } },
    }), /package/i);
    assert.equal(fs.existsSync(codexMarker), false);
    assert.equal(fs.existsSync(assayMarker), false);
  } finally { process.env.PATH = oldPath; }
});

test("#2823: retained package is rechecked by the consumer, including forged stored results", (t) => {
  const proofRoot = portableLiveProofRoot();
  t.after(() => fs.rmSync(proofRoot, { recursive: true, force: true }));
  const result = driveCli("valid", "failures", { captureMode: "host-observation", proofRoot });
  const manifest = JSON.parse(fs.readFileSync(path.join(proofRoot, "manifest.json"), "utf8"));
  assert.equal(manifest.schema, "assay.codex-host-proof.v6");
  const control = validateProofRoot(proofRoot);
  assert.equal(control.classified.cells.installPackageVerified.status, "pass", result.stderr);
  assert.equal(control.classified.cells.installRouteAdmissible.status, "pass");
  assert.equal(control.externalAttestation, "not_provided");
  const file = path.join(proofRoot, "assay-package.crate");
  const bytes = fs.readFileSync(file);
  fs.appendFileSync(file, "altered");
  const changed = validateProofRoot(proofRoot);
  assert.equal(changed.ok, false);
  assert.equal(changed.classified.cells.installPackageVerified.status, "fail");
  assert.ok(changed.reasons.includes("stored classification disagrees with recomputed classification"));
  fs.writeFileSync(file, bytes);
  assert.equal(validateProofRoot(proofRoot).classified.cells.installPackageVerified.status, "pass");
  manifest.packageVerification.binarySha256 = "0".repeat(64);
  fs.writeFileSync(path.join(proofRoot, "manifest.json"), stableStringify(manifest));
  assert.equal(validateProofRoot(proofRoot).classified.cells.installPackageVerified.status, "fail");
});

test("#2823: a package changed by the version probe cannot reach host spawn", (t) => {
  for (const changePackage of [false, true]) {
    const proofRoot = portableLiveProofRoot();
    const root = scratch();
    t.after(() => fs.rmSync(proofRoot, { recursive: true, force: true }));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const marker = path.join(root, "host-spawned");
    const codexBin = path.join(root, "codex");
    const projectRoot = seedProject();
    writePortableNodeExecutable(codexBin, `
import fs from "node:fs";
import { spawn } from "node:child_process";
if (process.argv.includes("--version")) {
  if (${changePackage}) fs.appendFileSync(${JSON.stringify(path.join(proofRoot, "assay-package.crate"))}, "changed after first check");
  process.stdout.write("codex-shadow/0.0.0\\n");
} else {
  fs.writeFileSync(${JSON.stringify(marker)}, "host started");
  const child = spawn(${JSON.stringify(process.execPath)}, ${JSON.stringify([FAKE, "--scenario", "valid", "--project-root", projectRoot])}, { stdio: "inherit" });
  const stop = () => { try { child.kill("SIGTERM"); } catch {} };
  process.on("SIGTERM", stop);
  child.on("close", code => process.exit(code ?? 1));
}
`);
    const run = driveCli("valid", "discovery", { captureMode: "host-observation", proofRoot, projectRoot, codexBin });
    assert.equal(fs.existsSync(marker), !changePackage,
      "the changed package must stop host spawn; the unchanged control must reach it");
    if (changePackage) {
      assert.notEqual(run.status, 0);
      assert.match(run.stderr, /package.*checksum|checksum.*mismatch/i);
    } else {
      assert.equal(validateProofRoot(proofRoot).classified.cells.installPackageVerified.status, "pass");
    }
  }
});

test("#2813: shared itemsView validation preserves the closed vocabulary and legacy absence", async () => {
  const { turnItemsViewReason } = await import("./codex_host_proof_validator.mjs");
  assert.equal(typeof turnItemsViewReason, "function");
  for (const view of [undefined, "full", "summary", "notLoaded"]) {
    assert.equal(turnItemsViewReason(view), null);
  }
  for (const view of [null, "", "partial", "Full", 0, false, [], {}, "PRIVATE_CANARY"]) {
    assert.equal(
      turnItemsViewReason(view),
      "turn/completed itemsView must be full, summary, or notLoaded",
    );
  }
});

test("#2813: invocation classification refuses an unknown itemsView in resolved topology", async () => {
  const control = await drive("valid");
  const topology = structuredClone(
    consumeJourneyTopology(control.events, control.manifest.journey),
  );
  assert.equal(topology.ok, true);
  const terminal = topology.notifications.find(
    (event) => event.method === "turn/completed" && event.direction === "server",
  );
  assert.ok(terminal, "resolved topology must contain the canonical terminal");
  terminal.params.turn.itemsView = "unknown_view";

  const cells = classifyCells(
    control.events,
    control.manifest,
    control.manifest.expected,
    control.manifest.journey,
    topology,
  );
  assert.deepEqual(cells.oneToolInvoked, {
    status: "fail",
    reason: "turn/completed itemsView must be full, summary, or notLoaded",
  });
});

function scratch() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "assay-2684-"));
}

// Declared install sources for CLI-level tests. Refusal tests need these too, so a
// refusal still proves the behaviour under test rather than the missing precondition.
const CLI_INSTALL_SOURCE_ARGS = Object.freeze([
  "--install-source", "codex", "host-bundled", "test-fixture-codex",
  "--install-source", "codexCodeModeHost", "host-bundled", "test-fixture-code-mode-host",
  "--install-source", "assayMcp", "crates-io", "assay-mcp-server@0.0.0-test",
]);

const TEST_INSTALL_SOURCES = Object.freeze({
  codex: { route: "host-bundled", reference: "test-fixture-codex" },
  codexCodeModeHost: { route: "host-bundled", reference: "test-fixture-code-mode-host" },
  assayMcp: { route: "crates-io", reference: "assay-mcp-server@0.0.0-test" },
});

// The in-process API bypasses parseArgs, so declared install sources are injected
// here the same way the CLI injects them through argv.
function runProof(options = {}) {
  const { installSources, ...rest } = options;
  return runProofProduction({
    ...rest,
    installSources: installSources ?? TEST_INSTALL_SOURCES,
  });
}

function resolveHostIdentity(options = {}) {
  const { installSources, proofRoot, ...rest } = options;
  return resolveHostIdentityProduction({
    ...rest,
    proofRoot: proofRoot ?? scratch(),
    installSources: installSources ?? TEST_INSTALL_SOURCES,
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

function wrapSpawnedStdinWrites(child) {
  const writesAfterClose = { count: 0 };
  const stdin = child.stdin;
  const nativeWrite = stdin.write.bind(stdin);
  stdin.write = function writeAfterClose(chunk, encoding, cb) {
    if (!stdin.writable || stdin.destroyed) {
      writesAfterClose.count += 1;
    }
    return nativeWrite(chunk, encoding, cb);
  };
  return writesAfterClose;
}

function writeMutatedDriver(mutate) {
  const dir = scratch();
  fs.writeFileSync(path.join(dir, "codex_host_proof.mjs"), mutate(DRIVER_SRC));
  fs.copyFileSync(
    path.join(HERE, "codex_host_proof_validator.mjs"),
    path.join(dir, "codex_host_proof_validator.mjs"),
  );
  return path.join(dir, "codex_host_proof.mjs");
}

async function drive(scenario, journey = "tool", options = {}) {
  const run = options.run ?? runProof;
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
  const child = spawnFakeChild(childArgv, projectRoot);
  const writesAfterClose = options.wrapStdin ? wrapSpawnedStdinWrites(child) : undefined;
  const result = await run({
    captureMode: "synthetic-fixture",
    timeoutMs: 4000,
    maxBytes: 1_048_576,
    journey,
    allowLiveTurn: false,
    testOnlyChild: child,
    proofRoot,
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
  });
  return { ...result, proofRoot, projectRoot, writesAfterClose };
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
  if (path.basename(filePath) === "codex") {
    writeCodeModeHostSibling(filePath);
  }
  return filePath;
}

function writeCodeModeHostSibling(codexPath) {
  const helperPath = path.join(path.dirname(codexPath), HOST_SUBJECTS[1]);
  if (!fs.existsSync(helperPath)) {
    fs.writeFileSync(helperPath, "#!/usr/bin/env node\n", { mode: 0o755 });
  }
  return helperPath;
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
  const prefix = scratch();
  fs.mkdirSync(path.join(prefix, "bin"));
  const bin = path.join(prefix, "bin", "assay-mcp-server");
  writePortableNodeExecutable(
    bin,
    `if (process.argv.includes("--version")) {
  process.stdout.write("assay-mcp-server 0.0.0-test\\n");
  process.exit(0);
}
process.stdout.write("assay-mcp-server 0.0.0-test\\n");
`,
  );
  const bytes = "synthetic package, never a published installation";
  fs.writeFileSync(path.join(prefix, "fixture.crate"), bytes);
  fs.writeFileSync(path.join(prefix, "index.json"), JSON.stringify({
    name: "assay-mcp-server", vers: "0.0.0-test", yanked: false, cksum: sha256Utf8(bytes),
  }));
  fs.writeFileSync(path.join(prefix, ".crates2.json"), JSON.stringify({ installs: {
    "assay-mcp-server 0.0.0-test (registry+https://github.com/rust-lang/crates.io-index)": {
      version_req: "=0.0.0-test", bins: ["assay-mcp-server"], profile: "release",
      target: "fixture-target", rustc: "fixture-rustc",
    },
  } }));
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
    ...CLI_INSTALL_SOURCE_ARGS,
  ];
  if (extra.captureMode === "host-observation") {
    const prefix = path.dirname(path.dirname(mcpBin));
    args.push("--assay-package", path.join(prefix, "fixture.crate"),
      "--assay-index-row", path.join(prefix, "index.json"),
      "--assay-cargo-metadata", path.join(prefix, ".crates2.json"));
  }
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
  assert.equal(manifest.schema, "assay.codex-host-proof.v6");
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

test("nonzero-after-success fixture waits for the elicitation acknowledgement", async () => {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const ackMarker = path.join(projectRoot, "elicitation-acknowledged");
  const child = spawnFakeChild(
    [
      "node",
      FAKE,
      "--scenario",
      "exit-1-after-success",
      "--project-root",
      projectRoot,
      "--ack-marker",
      ackMarker,
    ],
    projectRoot,
  );
  const result = await runProof({
    captureMode: "synthetic-fixture",
    timeoutMs: 4000,
    maxBytes: 1_048_576,
    journey: "tool",
    allowLiveTurn: false,
    testOnlyChild: child,
    proofRoot,
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
  });

  assert.equal(fs.readFileSync(ackMarker, "utf8"), "accept\n");
  assert.equal(result.childExitCode, 1);
  assert.equal(result.classified.cells.oneToolInvoked.status, "pass");
  assert.notEqual(result.driverOutcome.exitCode, 0);
});

test("nonzero-after-success fixture rejects a declined elicitation response", async () => {
  const projectRoot = seedProject();
  const ackMarker = path.join(projectRoot, "elicitation-acknowledged");
  const child = spawnFakeChild(
    [
      "node",
      FAKE,
      "--scenario",
      "exit-1-after-success",
      "--project-root",
      projectRoot,
      "--ack-marker",
      ackMarker,
    ],
    projectRoot,
  );
  const send = (message) => child.stdin.write(`${JSON.stringify(message)}\n`);
  send({ id: 1, method: "initialize", params: {} });
  send({ id: 2, method: "thread/start", params: { cwd: projectRoot } });
  send({ id: 3, method: "turn/start", params: { threadId: "thread-1", input: [] } });
  send({ id: "elicit-1", result: { action: "decline" } });

  const status = await new Promise((resolve) => child.on("close", resolve));
  assert.equal(status, 4);
  assert.equal(fs.existsSync(ackMarker), false);
});

test("closed stdin then elicitation reply is fail-closed, not uncaught EPIPE", async () => {
  const { classified, manifest, events, childExitCode, driverOutcome } = await drive(
    "close-stdin-then-elicit-exit-0",
  );
  assert.equal(
    clientParams(events, "mcpServer/elicitation/request").length,
    0,
    "must not retain a client elicitation reply that was not written",
  );
  assert.equal(manifest.streamUnavailable, true);
  assert.equal(driverOutcome.status, "unavailable");
  assert.notEqual(driverOutcome.exitCode, 0);
  assert.notEqual(driverOutcome.status, "pass");
  assert.equal(childExitCode, 0, "child exit 0 must remain visible so the I/O break is not a kill-fail");
  assert.notEqual(
    driverOutcome.status,
    "pass",
    "childExitCode 0 must never make the driver pass when stdin closed before elicitation",
  );
  assert.notEqual(classified.cells.oneToolInvoked.status, "pass");
  assert.notEqual(classified.externalAttestation, "pass");
});

test("mutation: removing the child-stdin write guard writes the closed pipe", async () => {
  const guardNeedle = `const childStdinIsWritable = () =>
    Boolean(
      !stopped &&
        childAlive &&
        child.stdin &&
        !child.stdin.destroyed &&
        child.stdin.writable &&
        !child.stdin.writableEnded,
    );`;
  const mutatedPath = writeMutatedDriver((src) => {
    assert.match(src, /const childStdinIsWritable = \(\) =>/);
    return src.replace(guardNeedle, "const childStdinIsWritable = () => true;");
  });
  const { runProof: runMutated } = await import(pathToFileURL(mutatedPath).href);
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const child = spawnFakeChild(
    ["node", FAKE, "--scenario", "valid", "--project-root", projectRoot],
    projectRoot,
  );
  const writesAfterClose = wrapSpawnedStdinWrites(child);
  child.stdin.destroy();
  const result = await runMutated({
    captureMode: "synthetic-fixture",
    timeoutMs: 4000,
    maxBytes: 1_048_576,
    journey: "discovery",
    allowLiveTurn: false,
    testOnlyChild: child,
    proofRoot,
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
  });
  assert.ok(writesAfterClose.count > 0, "unguarded write must hit closed stdin");
  assert.notEqual(result.driverOutcome.exitCode, 0);
});

test("mutation: retaining a client elicitation reply after a failed stdin write invents a sent event", async () => {
  const replyNeedle = "if (writeChildStdin(reply, () => dropClientWrite(message.id))) {";
  const mutatedPath = writeMutatedDriver((src) => {
    assert.match(src, /if \(writeChildStdin\(reply, \(\) => dropClientWrite\(message\.id\)\)\) \{/);
    return src.replace(replyNeedle, "if (writeChildStdin(reply, () => {}) || true) {");
  });
  const { runProof: runMutated } = await import(pathToFileURL(mutatedPath).href);
  const { events } = await drive("close-stdin-then-elicit-exit-0", "tool", { run: runMutated });
  assert.ok(
    clientParams(events, "mcpServer/elicitation/request").length > 0,
    "retaining a client elicitation after writeChildStdin false must invent a sent event",
  );
});

test("mutation: omitting streamUnavailable on stdin failure hides the I/O break", async () => {
  const noteNeedle = `    stdinFailureNoted = true;
    streamUnavailable = true;`;
  const mutatedPath = writeMutatedDriver((src) => {
    assert.match(src, /stdinFailureNoted = true;\n    streamUnavailable = true;/);
    return src.replace(noteNeedle, "    stdinFailureNoted = true;");
  });
  const { runProof: runMutated } = await import(pathToFileURL(mutatedPath).href);
  const { manifest, driverOutcome } = await drive("close-stdin-then-elicit-exit-0", "tool", { run: runMutated });
  assert.notEqual(manifest.streamUnavailable, true);
  assert.notEqual(driverOutcome.status, "unavailable");
});

test("mutation: removing the stdin error handler leaves EPIPE uncaught", () => {
  const handlerNeedle = `  if (child.stdin) {
    child.stdin.on("error", onChildStdinError);
  }
`;
  const mutatedPath = writeMutatedDriver((src) => {
    assert.match(src, /child\.stdin\.on\("error", onChildStdinError\)/);
    return src.replace(handlerNeedle, "");
  });
  const script = path.join(scratch(), "run-handler-mutation.mjs");
  fs.writeFileSync(
    script,
    `import { runProof } from ${JSON.stringify(pathToFileURL(mutatedPath).href)};
import { EventEmitter } from "node:events";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { PassThrough } from "node:stream";

const projectRoot = fs.mkdtempSync(path.join(os.tmpdir(), "assay-2684-"));
fs.mkdirSync(path.join(projectRoot, ".agents/skills/assay-golden-path"), { recursive: true });
fs.writeFileSync(path.join(projectRoot, ".agents/skills/assay-golden-path/SKILL.md"), "# disposable canary\\n");
const proofRoot = fs.mkdtempSync(path.join(os.tmpdir(), "assay-2684-"));
const stdin = new PassThrough();
const stdout = new PassThrough();
const stderr = new PassThrough();
const child = new EventEmitter();
child.stdin = stdin;
child.stdout = stdout;
child.stderr = stderr;
child.kill = () => child.emit("close", 1, null);
stdin.write = (_chunk, encoding, cb) => {
  const err = Object.assign(new Error("write EPIPE"), { errno: -32, code: "EPIPE", syscall: "write" });
  queueMicrotask(() => stdin.emit("error", err));
  if (typeof encoding === "function") encoding(err);
  else if (typeof cb === "function") cb(err);
  return false;
};
await runProof({
  captureMode: "synthetic-fixture",
  timeoutMs: 4000,
  maxBytes: 1048576,
  journey: "discovery",
  allowLiveTurn: false,
  testOnlyChild: child,
  proofRoot,
  projectRoot,
  assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
});
`,
  );
  const result = spawnSync(process.execPath, [script], { encoding: "utf8", timeout: 15_000 });
  assert.notEqual(result.status, 0, "removing the stdin error handler must fail the process");
  assert.match(`${result.stderr}\n${result.stdout}`, /EPIPE/);
});

test("delayed async EPIPE after elicitation-accept stdin write does not retain a client accept", async () => {
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const child = spawnFakeChild(
    ["node", FAKE, "--scenario", "valid", "--project-root", projectRoot],
    projectRoot,
  );
  const stdin = child.stdin;
  const nativeWrite = stdin.write.bind(stdin);
  stdin.write = function delayedEpipeOnAccept(chunk, encoding, cb) {
    const text = Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk);
    let parsed = null;
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = null;
    }
    const isAccept =
      parsed &&
      parsed.method == null &&
      parsed.result &&
      parsed.result.action === "accept";
    if (!isAccept) {
      return nativeWrite(chunk, encoding, cb);
    }
    const err = Object.assign(new Error("write EPIPE"), {
      errno: -32,
      code: "EPIPE",
      syscall: "write",
    });
    const callback = typeof encoding === "function" ? encoding : cb;
    queueMicrotask(() => {
      if (typeof callback === "function") {
        callback(err);
      }
      stdin.emit("error", err);
    });
    return true;
  };
  const result = await runProof({
    captureMode: "synthetic-fixture",
    timeoutMs: 4000,
    maxBytes: 1_048_576,
    journey: "tool",
    allowLiveTurn: false,
    testOnlyChild: child,
    proofRoot,
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
  });
  assert.equal(
    clientParams(result.events, "mcpServer/elicitation/request").length,
    0,
    "must not retain a client elicitation accept whose write later EPIPE'd",
  );
  assert.equal(result.manifest.streamUnavailable, true);
  assert.equal(result.driverOutcome.status, "unavailable");
  assert.notEqual(result.driverOutcome.exitCode, 0);
  const driverErrors = result.events.filter(
    (event) => event.direction === "driver" && event.method === "error",
  );
  assert.equal(driverErrors.length, 1, "callback-error and stdin error must share one idempotent sink");
});

test("stdin write() returning false (backpressure) is not unavailable", async () => {
  const { EventEmitter } = await import("node:events");
  const { PassThrough } = await import("node:stream");
  const projectRoot = seedProject();
  const proofRoot = scratch();
  const stdin = new PassThrough();
  const stdout = new PassThrough();
  const stderr = new PassThrough();
  const child = new EventEmitter();
  child.stdin = stdin;
  child.stdout = stdout;
  child.stderr = stderr;
  let closed = false;
  const closeChild = () => {
    if (closed) {
      return;
    }
    closed = true;
    child.emit("close", 0, null);
  };
  child.kill = () => closeChild();
  stdin.on("finish", closeChild);
  stdin.write = function backpressureWrite(chunk, encoding, cb) {
    const text = Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk);
    for (const line of text.split("\n")) {
      if (!line.trim()) {
        continue;
      }
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        continue;
      }
      if (message.method === "initialize") {
        stdout.write(
          `${JSON.stringify({ id: message.id, result: { userAgent: "backpressure-test" } })}\n`,
        );
      } else if (message.method === "skills/list") {
        stdout.write(`${JSON.stringify({ id: message.id, result: { data: [] } })}\n`);
      }
    }
    const callback = typeof encoding === "function" ? encoding : cb;
    if (typeof callback === "function") {
      queueMicrotask(() => callback());
    }
    return false;
  };
  const result = await runProof({
    captureMode: "synthetic-fixture",
    timeoutMs: 4000,
    maxBytes: 1_048_576,
    journey: "discovery",
    allowLiveTurn: false,
    testOnlyChild: child,
    proofRoot,
    projectRoot,
    assayMcpBin: path.join(projectRoot, "install/bin/assay-mcp-server"),
  });
  assert.notEqual(result.manifest.streamUnavailable, true);
  assert.equal(
    result.events.some((event) => event.direction === "driver" && event.method === "error"),
    false,
    "write() returning false must not invent a stdin-failure driver error",
  );
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
    ...CLI_INSTALL_SOURCE_ARGS,
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
    "classification.json",
    "events.json",
    "manifest.json",
    ...HOST_SUBJECTS,
  ].sort());
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

function preSpawnStateMutations(event, manifest) {
  return [
    { label: "extra event", events: [event, event], manifest },
    {
      label: "wrong child exit",
      events: [event],
      manifest: { ...manifest, childExitCode: 2 },
    },
    {
      label: "wrong driver outcome",
      events: [event],
      manifest: { ...manifest, driverOutcome: { exitCode: 1, status: "fail" } },
    },
    {
      label: "truncated stream",
      events: [event],
      manifest: { ...manifest, truncated: true },
    },
    {
      label: "available stream",
      events: [event],
      manifest: { ...manifest, streamUnavailable: false },
    },
    {
      label: "stdout bytes",
      events: [event],
      manifest: { ...manifest, bounds: { ...manifest.bounds, stdoutBytes: 1 } },
    },
    {
      label: "stderr bytes",
      events: [event],
      manifest: { ...manifest, bounds: { ...manifest.bounds, stderrBytes: 1 } },
    },
    {
      label: "initialize metadata",
      events: [event],
      manifest: { ...manifest, initialize: { ...manifest.initialize, userAgent: "spawned" } },
    },
  ];
}

test("production driver creates and verifies its disposable CODEX_HOME before spawn", () => {
  const projectRoot = seedProject();
  const codexHome = path.join(projectRoot, ".codex-home");
  const retainedFilesWithIdentity = [
    "classification.json",
    "events.json",
    "manifest.json",
    ...HOST_SUBJECTS,
  ].sort();
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
      method: "pre-spawn-error",
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

  const preSpawnResults = [];
  for (const row of [
    { journey: "discovery", allowLiveTurn: false },
    { journey: "failures", allowLiveTurn: false },
    { journey: "tool", allowLiveTurn: true },
  ]) {
    const hostProjectRoot = seedProject();
    fs.writeFileSync(path.join(hostProjectRoot, ".codex-home"), "not a directory\n");
    const hostProofRoot = portableLiveProofRoot();
    try {
      const hostCli = driveCli("valid", row.journey, {
        allowLiveTurn: row.allowLiveTurn,
        captureMode: "host-observation",
        projectRoot: hostProjectRoot,
        proofRoot: hostProofRoot,
        codexBin,
      });
      assert.notEqual(hostCli.status, 0);
      assert.deepEqual(
        proofFiles(hostCli.proofRoot),
        [...retainedFilesWithIdentity, "assay-package.crate", "assay-registry-row.json", "assay-install.json"].sort(),
        `host-observation ${row.journey} failure must retain its proof-owned subjects: ${hostCli.stderr || hostCli.stdout}`,
      );
      const checked = validateProofRoot(hostCli.proofRoot);
      preSpawnResults.push({ journey: row.journey, ok: checked.ok, reasons: checked.reasons });
      const manifest = JSON.parse(
        fs.readFileSync(path.join(hostCli.proofRoot, "manifest.json"), "utf8"),
      );
      const events = JSON.parse(
        fs.readFileSync(path.join(hostCli.proofRoot, "events.json"), "utf8"),
      );
      if (row.journey === "discovery") {
        const originalClassified = classifyRecord({ ...manifest, events });
        for (const mutation of preSpawnStateMutations(events[0], manifest)) {
          const mutatedClassified = classifyRecord({
            ...mutation.manifest,
            events: mutation.events,
          });
          rewriteProof(
            hostCli.proofRoot,
            mutation.manifest,
            mutation.events,
            mutatedClassified,
          );
          const mutatedChecked = validateProofRoot(hostCli.proofRoot);
          assert.equal(
            mutatedChecked.ok,
            false,
            `${mutation.label} must fail at the retained-proof validator funnel`,
          );
          rewriteProof(hostCli.proofRoot, manifest, events, originalClassified);
        }
      }
      if (row.journey !== "discovery") {
        const genericEvents = structuredClone(events);
        genericEvents[0].method = "error";
        const genericClassified = classifyRecord({ ...manifest, events: genericEvents });
        rewriteProof(hostCli.proofRoot, manifest, genericEvents, genericClassified);
        const genericChecked = validateProofRoot(hostCli.proofRoot);
        assert.equal(
          genericChecked.ok,
          false,
          `generic driver/error must not unlock ${row.journey} missing-command validation`,
        );
        assert.match(genericChecked.reasons.join("; "), /host subjects.*commands/);
      }
      assert.equal(
        fs.existsSync(path.join(hostProjectRoot, ".codex-app-server-started")),
        false,
        `host-observation ${row.journey} must reject unsafe CODEX_HOME before app-server spawn`,
      );
    } finally {
      fs.rmSync(hostProofRoot, { recursive: true, force: true });
    }
  }
  assert.deepEqual(
    preSpawnResults,
    [
      { journey: "discovery", ok: true, reasons: [] },
      { journey: "failures", ok: true, reasons: [] },
      { journey: "tool", ok: true, reasons: [] },
    ],
    JSON.stringify(preSpawnResults),
  );

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

test("pre-spawn failure is one closed unavailable state, not a generic stream exception", () => {
  const event = {
    direction: "driver",
    method: "pre-spawn-error",
    params: { message: "retained driver error" },
  };
  const manifest = {
    childExitCode: 1,
    driverOutcome: { exitCode: 1, status: "unavailable" },
    truncated: false,
    streamUnavailable: true,
    bounds: { stdoutBytes: 0, stderrBytes: 0 },
    initialize: {
      codexHome: null,
      userAgent: null,
      platformFamily: null,
      platformOs: null,
    },
  };
  assert.deepEqual(preSpawnFailureState([event], manifest), {
    present: true,
    valid: true,
    reason: null,
  });

  for (const mutation of preSpawnStateMutations(event, manifest)) {
    const state = preSpawnFailureState(mutation.events, mutation.manifest);
    assert.equal(state.present, true);
    assert.equal(state.valid, false);
    assert.match(state.reason, /^pre-spawn failure /);
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
      ...CLI_INSTALL_SOURCE_ARGS,
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

test("unavailable stream cannot retain a clean driver outcome", async () => {
  const { events, manifest, proofRoot } = await drive("valid");
  assert.equal(validateProofRoot(proofRoot).ok, true, "unaltered control must validate");

  const unavailable = {
    ...structuredClone(manifest),
    streamUnavailable: true,
  };
  const classified = classifyRecord({ ...unavailable, events });
  rewriteProof(proofRoot, unavailable, events, classified);

  const checked = validateProofRoot(proofRoot);
  assert.equal(
    checked.ok,
    false,
    "stream-unavailable evidence must reject a retained child/driver pass outcome",
  );
  assert.match(
    checked.reasons.join(" "),
    /childExitCode and driverOutcome violate the closed driver-outcome rule/,
  );
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
    assert.deepEqual(
      Object.keys(identity).sort(),
      ["arch", "assayMcp", "codex", "codexCodeModeHost", "os"],
    );
    for (const role of ["codex", "codexCodeModeHost", "assayMcp"]) {
      const expectedKeys = role === "codexCodeModeHost"
        ? ["installSource", "path", "sha256"]
        : ["installSource", "path", "sha256", "version"];
      assert.deepEqual(Object.keys(identity[role]).sort(), expectedKeys);
      assert.deepEqual(
        Object.keys(identity[role].installSource).sort(),
        ["reference", "route"],
      );
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
        ...CLI_INSTALL_SOURCE_ARGS,
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
    const codeModeHost = path.join(path.dirname(codexBin), HOST_SUBJECTS[1]);
    assert.equal(ignored.codex.sha256, sha256File(codexBin));
    assert.equal(ignored.codexCodeModeHost.sha256, sha256File(codeModeHost));
    assert.equal(ignored.assayMcp.sha256, sha256File(mcpBin));
    assert.equal(sha256File(ignored.codex.path), sha256File(codexBin));
    assert.equal(sha256File(ignored.codexCodeModeHost.path), sha256File(codeModeHost));
    assert.equal(sha256File(ignored.assayMcp.path), sha256File(mcpBin));
    assert.deepEqual(ignored.codex.installSource, TEST_INSTALL_SOURCES.codex);
    assert.deepEqual(ignored.assayMcp.installSource, TEST_INSTALL_SOURCES.assayMcp);
    assert.match(ignored.codex.version, /codex-shadow/);
    assert.equal(ignored.assayMcp.version, "assay-mcp-server 0.0.0-test");
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
      ...CLI_INSTALL_SOURCE_ARGS,
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
  const codeModeHost = path.join(path.dirname(codexBin), HOST_SUBJECTS[1]);
  assert.equal(manifest.hostIdentity.codex.sha256, sha256File(codexBin));
  assert.equal(manifest.hostIdentity.codexCodeModeHost.sha256, sha256File(codeModeHost));
  assert.equal(manifest.hostIdentity.assayMcp.sha256, sha256File(mcpBin));
  assert.equal(sha256File(manifest.hostIdentity.codex.path), sha256File(codexBin));
  assert.equal(sha256File(manifest.hostIdentity.assayMcp.path), sha256File(mcpBin));
  assert.deepEqual(
    Object.keys(manifest.hostIdentity).sort(),
    ["arch", "assayMcp", "codex", "codexCodeModeHost", "os"],
  );
  assert.deepEqual(
    Object.keys(manifest.hostIdentity.codex).sort(),
    ["installSource", "path", "sha256", "version"],
  );
  assert.deepEqual(
    Object.keys(manifest.hostIdentity.codexCodeModeHost).sort(),
    ["installSource", "path", "sha256"],
  );
  assert.deepEqual(
    Object.keys(manifest.hostIdentity.assayMcp).sort(),
    ["installSource", "path", "sha256", "version"],
  );
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

test("host identity refuses a Codex installation without its code-mode host", () => {
  const codex = writeVersionOnlyBin("codex", "codex-control/0.0.0");
  fs.rmSync(path.join(path.dirname(codex), HOST_SUBJECTS[1]));
  const mcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-control/0.0.0");
  const proofRoot = scratch();
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(codex)}${path.delimiter}${path.dirname(mcp)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(
      () => resolveHostIdentity({ proofRoot }),
      /codex-code-mode-host/i,
      "the proof must refuse before Codex can resolve an unbound sibling helper",
    );
    for (const subject of HOST_SUBJECTS) {
      assert.equal(
        fs.existsSync(path.join(proofRoot, subject)),
        false,
        `missing helper must not leave ${subject}`,
      );
    }
  } finally {
    process.env.PATH = previousPath;
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("code-mode host subject uses the platform runtime name", () => {
  assert.equal(
    HOST_SUBJECTS[1],
    process.platform === "win32" ? "codex-code-mode-host.exe" : "codex-code-mode-host",
  );
});

test("pre-existing proof subjects are refused without being deleted", () => {
  const proofRoot = scratch();
  const sentinel = path.join(proofRoot, HOST_SUBJECTS[1]);
  fs.writeFileSync(sentinel, "pre-existing\n", { mode: 0o700 });
  const codex = writeVersionOnlyBin("codex", "codex-control/0.0.0");
  const mcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-control/0.0.0");
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(codex)}${path.delimiter}${path.dirname(mcp)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(
      () => resolveHostIdentity({ proofRoot }),
      /already exists/i,
      "acquisition must refuse before touching an existing proof subject",
    );
    assert.equal(fs.readFileSync(sentinel, "utf8"), "pre-existing\n");
  } finally {
    process.env.PATH = previousPath;
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("partial acquisition removes every proof-owned subject", () => {
  const proofRoot = scratch();
  const codex = writeVersionOnlyBin("codex", "codex-control/0.0.0");
  const mcp = writeVersionOnlyBin("assay-mcp-server", "assay-mcp-control/0.0.0");
  const previousPath = process.env.PATH;
  process.env.PATH = `${path.dirname(codex)}${path.delimiter}${path.dirname(mcp)}${path.delimiter}${previousPath}`;
  try {
    assert.throws(
      () => resolveHostIdentity({
        proofRoot,
        testOnlyAfterSnapshotRead(copied, _src, destName) {
          if (destName === "assay-mcp-server.snapshot" && copied === 0) {
            throw new Error("injected third-subject failure");
          }
        },
      }),
      /injected third-subject failure/i,
    );
    for (const subject of HOST_SUBJECTS) {
      assert.equal(
        fs.existsSync(path.join(proofRoot, subject)),
        false,
        `partial acquisition must remove ${subject}`,
      );
    }
  } finally {
    process.env.PATH = previousPath;
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

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
    assert.deepEqual(control.codex.installSource, TEST_INSTALL_SOURCES.codex);
    assert.deepEqual(control.assayMcp.installSource, TEST_INSTALL_SOURCES.assayMcp);
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
  const bin = writeSparseFile(path.join(scratch(), name), size);
  if (name === "codex") {
    writeCodeModeHostSibling(bin);
  }
  return bin;
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
    assert.deepEqual(control.codex.installSource, TEST_INSTALL_SOURCES.codex);
    assert.deepEqual(control.assayMcp.installSource, TEST_INSTALL_SOURCES.assayMcp);
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
    for (const role of ["codex", "codexCodeModeHost", "assayMcp"]) {
      const relative = path.relative(proofRoot, manifest.hostIdentity[role].path);
      assert.equal(relative.startsWith("..") || path.isAbsolute(relative), false);
      assert.equal(fs.existsSync(manifest.hostIdentity[role].path), true);
    }
    fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
    fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    const events = JSON.parse(fs.readFileSync(path.join(proofRoot, "events.json"), "utf8"));
    const topology = consumeJourneyTopology(events, manifest.journey);
    assert.equal(sha256File(manifest.hostIdentity.codex.path), manifest.hostIdentity.codex.sha256);
    assert.equal(
      sha256File(manifest.hostIdentity.codexCodeModeHost.path),
      manifest.hostIdentity.codexCodeModeHost.sha256,
    );
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
    const wrongCommand = structuredClone(topology);
    wrongCommand.primaryThread.request.params.config.mcp_servers.assay.command =
      "/wrong/canonical/assay-mcp-server";
    assert.equal(
      verifyLiveIdentity(
        manifest.hostIdentity,
        manifest.invocation,
        wrongCommand,
        proofRoot,
        manifest.journey,
      ),
      false,
      "a normal journey cannot bypass the canonical MCP command binding",
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
    const codeModeHost = manifest.hostIdentity.codexCodeModeHost.path;
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

    const helperOriginal = fs.readFileSync(codeModeHost);
    fs.appendFileSync(codeModeHost, "\nmutated helper\n");
    rejects("altered code-mode host subject");
    fs.rmSync(codeModeHost, { force: true });
    fs.writeFileSync(codeModeHost, helperOriginal, { mode: 0o700 });
    assert.equal(validateProofRoot(proofRoot).ok, true, "restored helper control must validate");
  } finally {
    if (observed) {
      fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
      fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    }
    fs.rmSync(proofRoot, { recursive: true, force: true });
  }
});

test("review closeout: host identity is bound to the closed proof-root shape", () => {
  const proofRoot = portableLiveProofRoot();
  let observed;
  let outsideRoot;
  try {
    observed = driveCli("valid", "tool", {
      captureMode: "synthetic-fixture",
      proofRoot,
    });
    assert.equal(validateProofRoot(proofRoot).ok, true, "unchanged control must validate");
    const manifestPath = path.join(proofRoot, "manifest.json");
    const control = JSON.parse(fs.readFileSync(manifestPath, "utf8"));

    outsideRoot = scratch();
    const outsideHelper = path.join(outsideRoot, HOST_SUBJECTS[1]);
    fs.copyFileSync(control.hostIdentity.codexCodeModeHost.path, outsideHelper);
    fs.chmodSync(outsideHelper, 0o700);
    const rebound = structuredClone(control);
    rebound.hostIdentity.codexCodeModeHost.path = fs.realpathSync(outsideHelper);
    assert.equal(
      sha256File(outsideHelper),
      rebound.hostIdentity.codexCodeModeHost.sha256,
      "the path mutation must preserve the helper digest",
    );
    fs.writeFileSync(manifestPath, stableStringify(rebound));
    assert.equal(
      validateProofRoot(proofRoot).ok,
      false,
      "an identical helper outside the proof root must not satisfy path binding",
    );

    const widened = structuredClone(control);
    widened.hostIdentity.codexCodeModeHost.unexpected = true;
    fs.writeFileSync(manifestPath, stableStringify(widened));
    assert.equal(
      validateProofRoot(proofRoot).ok,
      false,
      "the helper identity must remain the exact path+sha256 projection",
    );
  } finally {
    if (observed) {
      fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
      fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    }
    if (outsideRoot) {
      fs.rmSync(outsideRoot, { recursive: true, force: true });
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
    for (const subject of HOST_SUBJECTS) {
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

test("host observation disables unrelated app and plugin surfaces before app-server", () => {
  const proofRoot = portableLiveProofRoot();
  const argvMarker = path.join(scratch(), "codex-argv.json");
  const codexBin = path.join(scratch(), "codex");
  const projectRoot = seedProject();
  writePortableNodeExecutable(
    codexBin,
    `import fs from "node:fs";
import { spawn } from "node:child_process";
if (process.argv.includes("--version")) {
  process.stdout.write("codex-shadow/0.0.0\\n");
  process.exit(0);
}
fs.writeFileSync(${JSON.stringify(argvMarker)}, JSON.stringify(process.argv.slice(2)));
const child = spawn(${JSON.stringify(process.execPath)}, ${JSON.stringify([
      FAKE,
      "--scenario",
      "valid",
      "--project-root",
      projectRoot,
    ])}, { stdio: "inherit" });
const stop = () => { try { child.kill("SIGTERM"); } catch { /* already exited */ } };
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
child.on("close", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
`,
  );
  const observed = driveCli("valid", "failures", {
    captureMode: "host-observation",
    proofRoot,
    projectRoot,
    codexBin,
  });
  try {
    const manifest = JSON.parse(
      fs.readFileSync(path.join(observed.proofRoot, "manifest.json"), "utf8"),
    );
    assert.deepEqual(manifest.invocation.argv, [
      manifest.hostIdentity.codex.path,
      "--disable",
      "apps",
      "--disable",
      "plugins",
      "--disable",
      "remote_plugin",
      "app-server",
    ]);
    assert.deepEqual(JSON.parse(fs.readFileSync(argvMarker, "utf8")), [
      "--disable",
      "apps",
      "--disable",
      "plugins",
      "--disable",
      "remote_plugin",
      "app-server",
    ]);
  } finally {
    fs.rmSync(path.dirname(observed.codexBin), { recursive: true, force: true });
    fs.rmSync(path.dirname(observed.mcpBin), { recursive: true, force: true });
    fs.rmSync(observed.proofRoot, { recursive: true, force: true });
    fs.rmSync(observed.projectRoot, { recursive: true, force: true });
  }
});

const OBSERVED_NON_EVIDENTIARY_ITEMS = Object.freeze([
  Object.freeze({
    type: "reasoning",
    id: "item_0",
    summary: [],
    content: [{ type: "text", text: "Considering the assay decide tool." }],
  }),
  Object.freeze({
    type: "agentMessage",
    id: "item_2",
    phase: "final_answer",
    text: "The decision endpoint returned allow.",
    delivery: "final",
    memoryCitation: null,
  }),
  Object.freeze({
    type: "commandExecution",
    id: "exec_1",
    command: "ls -la /private/tmp",
    commandActions: [{ type: "read", path: "/private/tmp" }],
    cwd: "/private/tmp",
    status: "completed",
    aggregatedOutput: "sensitive directory output",
    durationMs: 42,
    exitCode: 0,
    pluginId: null,
    processId: "1234",
    scriptPath: null,
    source: "agent",
  }),
]);

const OBSERVED_NON_EVIDENTIARY_NOTIFICATIONS = Object.freeze([
  "account/rateLimits/updated",
  "item/agentMessage/delta",
  "thread/tokenUsage/updated",
  "serverRequest/resolved",
]);

test("stableStringify refuses non-JSON primitives; JSON null stays a valid token", () => {
  assert.equal(stableStringify({ a: null }), '{"a":null}\n');
  assert.equal(stableStringify([null, 1, "x", true]), '[null,1,"x",true]\n');
  for (const bad of [
    undefined,
    { server: undefined },
    [undefined],
    { nested: { deep: undefined } },
    { fn: () => {} },
  ]) {
    assert.throws(
      () => stableStringify(bad),
      /JSON/,
      "a value JSON cannot encode must be rejected, never serialized as a bare token",
    );
  }
});

test("known non-evidentiary items project to a closed type and id only", () => {
  const projected = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5,
      threadId: "t",
      turnId: "u",
      item: OBSERVED_NON_EVIDENTIARY_ITEMS[0],
    },
  });
  const item = projected.params.item;
  assert.deepEqual(item, { type: "reasoning", id: "item_0" });
  const serialized = stableStringify(projected);
  assert.doesNotMatch(
    serialized,
    /[:,[]undefined/,
    "a bare undefined token must never appear; the quoted marker string is the valid form",
  );
  assert.equal(
    serialized.includes("Considering the assay decide tool."),
    false,
    "reasoning text must not be retained",
  );
  assert.deepEqual(JSON.parse(serialized), projected, "retained bytes must round-trip");
  const withNull = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5,
      threadId: "t",
      turnId: "u",
      item: { type: "userMessage", id: null, content: [] },
    },
  });
  assert.equal(withNull.params.item.id, null, "JSON null must survive the scalar boundary");
});

test("current Codex non-evidentiary items are dropped and do not invalidate a real tool row", async () => {
  const control = await drive("valid");
  assert.equal(validateProofRoot(control.proofRoot).ok, true, "unchanged control must validate");
  const events = structuredClone(control.events);
  const toolIndex = events.findIndex(
    (event) => event.method === "item/completed" && event.params?.item?.type === "mcpToolCall",
  );
  assert.notEqual(toolIndex, -1, "control run must contain the tool item to replace");
  const toolRow = events[toolIndex];
  const chatterRows = OBSERVED_NON_EVIDENTIARY_ITEMS.map((item) =>
    projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: toolRow.params.completedAtMs,
        threadId: toolRow.params.threadId,
        turnId: toolRow.params.turnId,
        item,
      },
    }),
  );
  const lifecycleRows = OBSERVED_NON_EVIDENTIARY_NOTIFICATIONS.map((method) =>
    projectRetainedEvent({
      direction: "server",
      id: null,
      method,
      params: { sensitiveHostValue: "must-not-be-retained" },
    }),
  );
  for (const row of lifecycleRows) {
    assert.deepEqual(row.params, {}, `${row.method} payload must be scrubbed`);
    assert.equal(classifyStoredEvent(row).type, "server-notification");
  }
  events.splice(toolIndex, 0, ...chatterRows, ...lifecycleRows);
  const terminalIndex = events.findIndex((event) => event.method === "turn/completed");
  assert.notEqual(terminalIndex, -1, "control run must contain the terminal turn");
  const terminal = events[terminalIndex];
  events[terminalIndex] = projectRetainedEvent({
    direction: "server",
    method: "turn/completed",
    id: null,
    params: {
      threadId: terminal.params.threadId,
      turn: {
        id: terminal.params.turn.id,
        status: terminal.params.turn.status,
        items: [
          ...terminal.params.turn.items,
          ...OBSERVED_NON_EVIDENTIARY_ITEMS,
        ],
      },
    },
  });
  const serialized = stableStringify(events);
  const parsed = JSON.parse(serialized);
  assert.deepEqual(parsed, events, "retained drift events must round-trip through JSON");
  assert.equal(serialized.includes("The decision endpoint returned allow."), false);
  assert.equal(serialized.includes("Considering the assay decide tool."), false);
  const notificationItem = chatterRows[0].params.item;
  const terminalItems = events[terminalIndex].params.turn.items;
  const terminalReasoning = terminalItems.find((item) => item.type === "reasoning");
  assert.deepEqual(
    terminalReasoning,
    notificationItem,
    "item/completed and terminal items must share one projection rule",
  );
  const classified = classifyRecord({ ...control.manifest, events });
  assert.equal(classified.cells.oneToolInvoked.status, "pass");
  assert.equal(classified.cells.structuredResultValidated.status, "pass");
  rewriteProof(control.proofRoot, control.manifest, events, classified);
  const revalidated = validateProofRoot(control.proofRoot);
  assert.equal(revalidated.ok, true, "known scrubbed chatter must validate with the tool row");
});

test("zero retained mcpToolCall rows keep oneToolInvoked non-pass", async () => {
  const control = await drive("valid");
  const events = structuredClone(control.events).filter(
    (event) =>
      !(event.method === "item/completed" && event.params?.item?.type === "mcpToolCall"),
  );
  const terminal = events.find((event) => event.method === "turn/completed");
  assert.ok(terminal, "control run must contain the terminal turn");
  terminal.params.turn.items = terminal.params.turn.items.filter(
    (item) => item?.type !== "mcpToolCall",
  );
  const classified = classifyRecord({ ...control.manifest, events });
  assert.notEqual(classified.cells.oneToolInvoked.status, "pass");
  assert.match(classified.cells.oneToolInvoked.reason, /no mcpToolCall/);
  assert.notEqual(classified.cells.structuredResultValidated.status, "pass");
});

// Reaches projectRetainedItem's type projection through the exported entry point, so the assertion
// is on production behaviour rather than on a re-implementation of the rule.
function projectRetainedItem_typeOf(rawType) {
  const projected = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 1,
      threadId: "t-1",
      turnId: "u-1",
      item: {
        type: rawType,
        id: "item-1",
        server: "assay",
        tool: "unmapped",
        arguments: { tool: "x", policy: "y" },
        status: "completed",
      },
    },
  });
  return projected.params.item.type;
}

test("projection-idempotent unknown terminal items are drift, not accepted topology", async () => {
  const control = await drive("valid");
  // An unknown item type is now an explicit projection violation, not a legal-looking substitute,
  // so it is refused at the consumer and can no longer reach the cells at all. That is strictly
  // stronger than the drift path this test used to probe, so the probe is re-aimed at the barrier.
  for (const raw of ["futureHostItem", "not-a-retained-item"]) {
    const projectedType = projectRetainedItem_typeOf(raw);
    assert.equal(
      typeof projectedType,
      "object",
      `${raw} must become an explicit violation, never a legal-looking value`,
    );
    assert.equal(
      JSON.stringify(projectedType).includes(raw),
      false,
      `${raw} must not survive inside the violation marker`,
    );
  }
  // Past the projection barrier: a terminal item that is well-formed EXCEPT for carrying a
  // violation must still be refused by the cells and by revalidation, not merely marked. The
  // crafted item is projected first, so what is classified is what a real run would have persisted
  // -- this is the "probe past the round-trip" the test exists for, re-expressed now that an
  // unknown type cannot survive projection at all.
  for (const label of ["unknown-type", "undeclared-tool"]) {
    const events = structuredClone(control.events);
    const terminal = events.find((event) => event.method === "turn/completed");
    assert.ok(terminal, "control run must contain the terminal turn");
    terminal.params.turn.items.push({
      type: label === "unknown-type" ? "futureHostItem" : "mcpToolCall",
      id: `item_${label}`,
      server: "assay",
      tool: label === "undeclared-tool" ? "unmapped" : "assay_policy_decide",
      arguments: { tool: "x", policy: "y" },
      status: "completed",
    });
    const projected = projectRetainedEvent(terminal);
    const classified = classifyRecord({ ...control.manifest, events: [...events.slice(0, -1), projected] });
    assert.equal(
      allIntendedCellsPass(classified),
      false,
      `a ${label} terminal item must not classify clean`,
    );
    const reasons = Object.values(classified.cells)
      .map((entry) => entry.reason)
      .join("; ");
    // Group E: the refusal must name the condition and never echo the host value.
    for (const hostValue of ["futureHostItem", "unmapped"]) {
      assert.equal(
        reasons.includes(hostValue),
        false,
        `${label}: a refusal reason must not echo the host value ${hostValue}`,
      );
    }
  }
});

test("non-finite numbers never silently become JSON null at either boundary", () => {
  for (const bad of [NaN, Infinity, -Infinity]) {
    assert.throws(
      () => stableStringify({ completedAtMs: bad }),
      /JSON/,
      `${bad} must fail loud before persistence, never serialize as null`,
    );
    assert.throws(() => stableStringify([bad]), /JSON/);
    const projected = projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: bad,
        threadId: "t",
        turnId: "u",
        item: { type: "userMessage", id: "u1", content: [] },
      },
    });
    assert.deepEqual(
      projected.params.completedAtMs,
      { __invalidType: "number" },
      `${bad} must project to the explicit invalid marker, not travel as a number`,
    );
    const serialized = stableStringify(projected);
    assert.deepEqual(JSON.parse(serialized), projected, "the marker form must round-trip");
  }
  assert.equal(stableStringify({ completedAtMs: 5 }), '{"completedAtMs":5}\n');
  assert.equal(stableStringify([0, -1.5]), '[0,-1.5]\n');
});

test("commandExecution projects to closed type and id only; sensitive fields are dropped, not scrubbed", () => {
  const rawItem = OBSERVED_NON_EVIDENTIARY_ITEMS.find((item) => item.type === "commandExecution");
  assert.ok(rawItem, "fixture must contain commandExecution");
  const projected = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 12345,
      threadId: "t-1",
      turnId: "u-1",
      item: rawItem,
    },
  });
  assert.deepEqual(projected.params.item, {
    type: "commandExecution",
    id: "exec_1",
  });
  const serialized = stableStringify(projected);
  assert.equal(serialized.includes("sensitive directory output"), false);
  assert.equal(serialized.includes("ls -la"), false);
  assert.equal(serialized.includes("/private/tmp"), false);

  const unprojectedRaw = {
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 12345,
      threadId: "t-1",
      turnId: "u-1",
      item: rawItem,
    },
  };
  const classified = classifyStoredEvent(unprojectedRaw);
  assert.equal(
    classified.type,
    "unclassified",
    "unprojected commandExecution containing raw fields must fail closed",
  );
});

test("Codex 0.153.1 mcpToolCall schema fields type-check; free text is recorded as presence and unexpected keys are marked", () => {
  const rawToolCall = {
    type: "mcpToolCall",
    id: "tool-1",
    server: "assay",
    tool: "assay_policy_decide",
    status: "completed",
    arguments: { tool: "install_surface_allowed_probe", policy: "install-surface-policy.yaml" },
    durationMs: 120,
    readOnlyHint: true,
    pluginId: "plugin-assay",
    mcpAppResourceUri: "resource://assay",
    appContext: {
      connectorId: "connector-1",
      linkId: "link-1",
      resourceUri: "uri://assay-policy",
    },
    error: {
      message: "authorization failed for token sk-secret12345678",
    },
    result: {
      isError: false,
      structuredContent: { allowed: true, reason: "ok" },
    },
  };

  const projected = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: rawToolCall,
    },
  });
  const projectedItem = projected.params.item;
  assert.equal(Object.hasOwn(projectedItem, "__unexpectedKeys"), false);
  assert.equal(projectedItem.durationMs, 120);
  assert.equal(projectedItem.readOnlyHint, true);
  assert.equal(projectedItem.pluginId, "[present]");
  assert.equal(projectedItem.mcpAppResourceUri, "[present]");
  assert.deepEqual(projectedItem.appContext, {
    connectorId: "[present]",
    linkId: "[present]",
    resourceUri: "[present]",
  });
  assert.equal(
    projectedItem.error.message,
    "[present]",
    "error message must record presence, not content: scrub() is a keyword regex and cannot bound arbitrary secret text",
  );

  const unexpectedKeyCall = structuredClone(rawToolCall);
  unexpectedKeyCall.unknownHostMetadata = "unexpected";
  const projectedWithUnexpected = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: unexpectedKeyCall,
    },
  });
  assert.equal(
    projectedWithUnexpected.params.item.__unexpectedKeys,
    "[present]",
    "the marker records that unexpected keys existed, never which",
  );
  assert.equal(
    JSON.stringify(projectedWithUnexpected).includes("unknownHostMetadata"),
    false,
    "the unexpected key NAME must never reach retained bytes",
  );
});

test("failed MCP tool status fails oneToolInvoked and structuredResultValidated", async () => {
  const control = await drive("valid");
  const events = structuredClone(control.events);
  const toolIndex = events.findIndex(
    (event) => event.method === "item/completed" && event.params?.item?.type === "mcpToolCall",
  );
  assert.notEqual(toolIndex, -1);
  events[toolIndex].params.item.status = "failed";
  events[toolIndex].params.item.result = null;

  const terminal = events.find((event) => event.method === "turn/completed");
  const terminalTool = terminal.params.turn.items.find(
    (item) => item?.type === "mcpToolCall",
  );
  if (terminalTool) {
    terminalTool.status = "failed";
    terminalTool.result = null;
  }

  const classified = classifyRecord({ ...control.manifest, events });
  assert.equal(classified.cells.oneToolInvoked.status, "fail");
  // The reason names the condition, never the host-supplied value (group E).
  assert.match(classified.cells.oneToolInvoked.reason, /status is not completed/);
  assert.equal(
    classified.cells.oneToolInvoked.reason.includes("failed"),
    false,
    "a refusal reason must not echo the host status value",
  );
  assert.equal(classified.cells.structuredResultValidated.status, "unavailable");
});

test("canonical Codex 0.153.1 elicitation prompt is accepted; unexpected prompt is declined", () => {
  const canonical01531 = {
    serverName: "assay",
    threadId: "t-1",
    turnId: "u-1",
    mode: "form",
    message: 'Allow the assay MCP server to run tool "assay_policy_decide"?',
    requestedSchema: { type: "object", properties: {} },
  };
  assert.equal(
    elicitationAcceptable(canonical01531, "t-1", "u-1"),
    true,
    "canonical 0.153.1 elicitation message must be accepted",
  );

  const syntheticLegacy = {
    serverName: "assay",
    threadId: "t-1",
    turnId: "u-1",
    mode: "form",
    message: "approve assay_policy_decide",
    requestedSchema: { type: "object", properties: {} },
  };
  assert.equal(
    elicitationAcceptable(syntheticLegacy, "t-1", "u-1"),
    true,
    "legacy synthetic elicitation message must remain accepted",
  );

  const unexpectedPrompt = {
    ...canonical01531,
    message: 'Allow the assay MCP server to run tool "unrelated_tool"?',
  };
  assert.equal(
    elicitationAcceptable(unexpectedPrompt, "t-1", "u-1"),
    false,
    "elicitation for an unpinned tool must be declined",
  );
});

test("classifyRecord rejects invalid 0.153.1 metadata types at the consumer boundary", async () => {
  const control = await drive("valid");
  assert.equal(allIntendedCellsPass(control.classified), true, "control must pass all cells");

  const badMetadataVariants = [
    { durationMs: "wrong" },
    { durationMs: -1 },
    { durationMs: 1.5 },
    { readOnlyHint: 42 },
    { readOnlyHint: "true" },
    { pluginId: false },
    { pluginId: 123 },
    { mcpAppResourceUri: true },
    { error: {} },
    { error: { message: 123 } },
    { appContext: {} },
    { appContext: { connectorId: 123 } },
    { durationMs: "wrong", readOnlyHint: 42, pluginId: false, error: {} },
  ];

  for (const badMeta of badMetadataVariants) {
    const events = structuredClone(control.events);
    const itemCompleted = events.find(
      (e) => e.method === "item/completed" && e.params?.item?.type === "mcpToolCall",
    );
    assert.ok(itemCompleted);
    Object.assign(itemCompleted.params.item, badMeta);

    const terminal = events.find((e) => e.method === "turn/completed");
    const terminalTool = terminal.params.turn.items.find(
      (item) => item?.type === "mcpToolCall",
    );
    assert.ok(terminalTool);
    Object.assign(terminalTool, badMeta);

    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(
      allIntendedCellsPass(classified),
      false,
      `metadata ${JSON.stringify(badMeta)} must not produce an all-pass classification`,
    );
  }
});

test("mcpToolCall with non-null error fails oneToolInvoked even if status claims completed", async () => {
  const control = await drive("valid");
  const events = structuredClone(control.events);
  const itemCompleted = events.find(
    (e) => e.method === "item/completed" && e.params?.item?.type === "mcpToolCall",
  );
  itemCompleted.params.item.error = { message: "unexpected error" };

  const terminal = events.find((e) => e.method === "turn/completed");
  const terminalTool = terminal.params.turn.items.find(
    (item) => item?.type === "mcpToolCall",
  );
  terminalTool.error = { message: "unexpected error" };

  const classified = classifyRecord({ ...control.manifest, events });
  assert.notEqual(
    classified.cells.oneToolInvoked.status,
    "pass",
    "mcpToolCall with error must not pass oneToolInvoked",
  );
  assert.equal(
    classified.cells.structuredResultValidated.status,
    "unavailable",
  );
});

test("F1: retained mcpToolCall metadata records presence, never host-supplied content", () => {
  // The producer of retained bytes must not write free-text host metadata verbatim. These fields
  // have no evidentiary consumer: no classification cell reads their content. Retaining presence
  // preserves every check that does exist while removing the only reason secrets can land in the
  // proof. scrub() is a keyword regex and cannot bound arbitrary secret text, so error.message is
  // held to the same rule rather than scrubbed.
  const secrets = {
    connectorId: "conn-alice@corp.example",
    actionName: "read /Users/alice/Documents/salary.xlsx",
    appName: "Private App",
    linkId: "https://example.invalid/cb?access_token=eyJhbGciOi.J9.sig",
    resourceUri: "https://example.invalid/r?bearer=eyJhbGciOi.J9.sig",
  };
  const projected = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: {
        type: "mcpToolCall",
        id: "tool-1",
        server: "assay",
        tool: "assay_policy_decide",
        status: "completed",
        arguments: { tool: "install_surface_allowed_probe" },
        pluginId: "/Users/alice/.config/plugins/private-plugin",
        mcpAppResourceUri: "https://example.invalid/a?bearer=eyJhbGciOi.J9.sig",
        appContext: secrets,
        error: { message: "failed reading /Users/alice/Documents/salary.xlsx as alice@corp.example" },
        result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
      },
    },
  });
  const blob = JSON.stringify(projected);
  for (const leaked of [
    ...Object.values(secrets),
    "/Users/alice/.config/plugins/private-plugin",
    "https://example.invalid/a?bearer=eyJhbGciOi.J9.sig",
    "/Users/alice/Documents/salary.xlsx",
    "alice@corp.example",
  ]) {
    assert.equal(
      blob.includes(leaked),
      false,
      `retained projection must not contain host-supplied content: ${leaked}`,
    );
  }
  // Presence is still recorded, so the cells that count fields keep working.
  const item = projected.params.item;
  assert.equal(item.pluginId, "[present]");
  assert.equal(item.mcpAppResourceUri, "[present]");
  assert.deepEqual(item.appContext, {
    connectorId: "[present]",
    actionName: "[present]",
    appName: "[present]",
    linkId: "[present]",
    resourceUri: "[present]",
  });
  assert.deepEqual(item.error, { message: "[present]" });
});

test("F2: a marked unexpected key is refused by the consumer, not merely marked", () => {
  // Marking the key in the projection is not refusal. classifyRecord is the consumer that decides
  // the nine cells; it must reject. Deleting the allowedKeys loop leaves the projection marking
  // intact, so only a consumer-level assertion kills that deletion.
  const projected = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: {
        type: "mcpToolCall",
        id: "tool-1",
        server: "assay",
        tool: "assay_policy_decide",
        status: "completed",
        arguments: { tool: "install_surface_allowed_probe" },
        unknownHostMetadata: "leak-me",
        result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
      },
    },
  });
  assert.equal(projected.params.item.__unexpectedKeys, "[present]");
  assert.equal(
    JSON.stringify(projected).includes("unknownHostMetadata"),
    false,
    "the unexpected key NAME must never reach retained bytes",
  );
  assert.equal(
    classifyStoredEvent(projected).type,
    "unclassified",
    "an item carrying an unexpected key must be refused by the consumer",
  );
});

// --- F1: the PRODUCER half of every new type check must bite -------------------------------
// The pre-existing consumer test injects malformed values onto ALREADY-PROJECTED bytes, so it
// exercises retainedItemReason only and never projectRetainedItem. These cases start from what a
// HOST would emit, run it through the producer, and assert the consumer refuses the result -- which
// is the only way a producer-side type check can be pinned.
for (const [label, item] of [
  ["pluginId non-string", { pluginId: 123 }],
  ["mcpAppResourceUri non-string", { mcpAppResourceUri: 7 }],
  ["durationMs non-number", { durationMs: "120" }],
  ["durationMs negative", { durationMs: -1 }],
  ["readOnlyHint non-boolean", { readOnlyHint: "true" }],
  ["appContext non-object", { appContext: 5 }],
  ["appContext missing connectorId", { appContext: {} }],
  ["appContext connectorId non-string", { appContext: { connectorId: 123 } }],
  ["appContext connectorId empty", { appContext: { connectorId: "" } }],
  ["appContext optional non-string", { appContext: { connectorId: "c", appName: 9 } }],
  ["error non-object", { error: 5 }],
  ["error message non-string", { error: { message: 9 } }],
]) {
  test(`F1 producer: host-emitted ${label} is refused after projection`, () => {
    const projected = projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "install_surface_allowed_probe" },
          result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
          ...item,
        },
      },
    });
    assert.equal(
      classifyStoredEvent(projected).type,
      "unclassified",
      `${label}: producer must not coerce a malformed host value into an acceptable projection`,
    );
  });
}

// --- F2: nested unknown keys must be marked AND refused ------------------------------------
// The refusal is indirect: the nested reason function rejects because the projection's
// __unexpectedKeys marker is itself outside its allowed list. Nothing stated that coupling, so
// deleting either half left the suite green. These pin both halves and the no-leak property.
for (const [label, item, probe] of [
  [
    "appContext",
    {
      appContext: { connectorId: "c-1", unknownHostNested: "F2-APPCONTEXT-LEAK" },
      result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
    },
    "F2-APPCONTEXT-LEAK",
  ],
  [
    "error",
    {
      error: { message: "boom", unknownHostNested: "F2-ERROR-LEAK" },
      result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
    },
    "F2-ERROR-LEAK",
  ],
]) {
  test(`F2 nested: an unknown key inside ${label} is marked and refused, and neither its name nor its value is retained`, () => {
    const projected = projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "install_surface_allowed_probe" },
          ...item,
        },
      },
    });
    const nested = projected.params.item[label];
    assert.equal(
      nested.__unexpectedKeys,
      "[present]",
      `${label}: the producer must mark that an unknown nested key existed`,
    );
    assert.equal(
      JSON.stringify(projected).includes("unknownHostNested"),
      false,
      `${label}: the unknown nested key NAME must never reach retained bytes`,
    );
    assert.equal(
      classifyStoredEvent(projected).type,
      "unclassified",
      `${label}: the consumer must refuse a marked nested key, not merely record it`,
    );
    assert.equal(
      JSON.stringify(projected).includes(probe),
      false,
      `${label}: the unknown nested value must never reach retained bytes`,
    );
  });
}

// --- F5: the canonical 0.153.1 prompt is accepted end-to-end, and legacy still is ----------
test("F5: the canonical elicitation prompt is accepted by a driven run", async () => {
  const canonical = await drive("canonical-elicitation-prompt");
  assert.equal(canonical.driverOutcome.exitCode, 0, "canonical prompt must drive to success");
  assert.equal(canonical.driverOutcome.status, "pass");
  const elicit = canonical.events.find(
    (e) => e.method === "mcpServer/elicitation/request" && e.direction === "server",
  );
  assert.ok(elicit, "the driven run must contain an elicitation request");
  assert.equal(
    elicit.params.message,
    'Allow the assay MCP server to run tool "assay_policy_decide"?',
    "the driven run must carry the canonical prompt, not the legacy one",
  );
});

// --- The closed whitelist vs the 0.153.1 schema's mcpToolCall property set ------------------
// allowedKeys is a CLOSED whitelist: a real host field missing from it refuses the whole proof.
// The generated protocol-0.153.1 schema is the only local statement of that set. It is a schema,
// not a live invocation claim -- a field can be declared and never emitted, and emission is not
// established here -- but a mismatch is actionable: a schema property absent
// from allowedKeys is a guaranteed false refusal, and an allowedKey absent from the schema admits
// something the host is not declared to send. Measured 2026-09-05 against
// protocol-0.153.1/v2/ThreadStartResponse.json (sha256
// 656f8fd0fe91f533126cbfdb9369cfab550927a229e48dd46460f6f015d2b186), definitions/ThreadItem/oneOf/8
// (McpToolCallThreadItem, 13 properties, no allOf/anyOf/$ref composition, additionalProperties unset).
// That file is not vendored here; this list is the pinned copy of what it declared when measured.
test("mcpToolCall projection accepts every declared 0.153.1 schema property", () => {
  const schemaProperties = [
    "appContext",
    "arguments",
    "durationMs",
    "error",
    "id",
    "mcpAppResourceUri",
    "pluginId",
    "readOnlyHint",
    "result",
    "server",
    "status",
    "tool",
    "type",
  ];
  // Derived from behaviour, not from importing the constant: a key the whitelist accepts produces
  // no top-level __unexpectedKeys marker, and one it rejects does.
  const project = (extraKey) =>
    projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "install_surface_allowed_probe" },
          result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
          ...(extraKey === null ? {} : { [extraKey]: null }),
        },
      },
    }).params.item;

  for (const key of schemaProperties) {
    const item = project(key);
    assert.equal(
      Object.hasOwn(item, "__unexpectedKeys"),
      false,
      `schema property ${key} must not be treated as unexpected: a closed whitelist that omits it refuses every real host record`,
    );
  }
  const control = project("definitelyNotInTheSchema");
  assert.equal(
    control.__unexpectedKeys,
    "[present]",
    "control: a key outside the schema must still be marked",
  );
  assert.equal(
    JSON.stringify(control).includes("definitelyNotInTheSchema"),
    false,
    "control: marking must not retain the name",
  );
});

// --- F-A: the whitelist must equal the schema set, not merely contain it -------------------
// The sibling test asserts only that the schema's declared properties are accepted. That leaves
// the other direction open: adding a key the schema does not declare survived it. A closed
// whitelist that admits an undeclared field is the mirror of one that omits a declared one -- it
// accepts something 0.153.1 never says the host sends.
//
// SCOPE: this is a SAMPLE, not a proof of set equality. It refuses the nine names below; it does
// not and cannot establish that every name outside the schema is refused, because it does not
// enumerate that set. Deliberately no parser and no generated name space here -- the sample is
// chosen to bite the realistic mutation (widening the whitelist), not to make a universal claim.
test("mcpToolCall projection refuses a sample of names the 0.153.1 schema does not declare", () => {
  // Names deliberately outside the 13 declared properties, including several that are plausible
  // near-misses of real fields elsewhere in the protocol.
  const notInSchema = [
    "reason",
    "content",
    "text",
    "output",
    "sessionId",
    "aggregatedOutput",
    "command",
    "cwd",
    "notInTheSchemaAtAll",
  ];
  for (const key of notInSchema) {
    const item = projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "install_surface_allowed_probe" },
          result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
          [key]: null,
        },
      },
    }).params.item;
    assert.equal(
      item.__unexpectedKeys,
      "[present]",
      `${key} is not declared by the 0.153.1 schema and must be marked unexpected`,
    );
    assert.equal(
      Object.hasOwn(item, key),
      false,
      `${key} must be marked without retaining its name as a key`,
    );
    assert.equal(
      JSON.stringify(item.__unexpectedKeys).includes(key),
      false,
      `${key} must not survive inside the marker`,
    );
  }
  // Control: a declared property must still be accepted, so the assertion above cannot pass by
  // marking everything.
  const declared = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: {
        type: "mcpToolCall",
        id: "tool-1",
        server: "assay",
        tool: "assay_policy_decide",
        status: "completed",
        arguments: { tool: "install_surface_allowed_probe" },
        result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
        durationMs: 120,
      },
    },
  }).params.item;
  assert.equal(Object.hasOwn(declared, "__unexpectedKeys"), false, "control: a declared property must be accepted");
});

// --- F-B: the producer's non-string barrier must hold for NON-SCALARS ----------------------
// Every existing case at these positions uses a scalar non-string, and the consumer's own type
// check catches scalars -- so the producer barrier is never the deciding check and its deletion
// went unnoticed. An object or array is the input where the producer is the only thing standing
// between host content and the retained bytes: without it the value is written verbatim BEFORE
// the consumer refuses the record.
for (const [label, patch, marker] of [
  ["pluginId object", { pluginId: { secret: "FB-PLUGIN-OBJ" } }, "FB-PLUGIN-OBJ"],
  ["mcpAppResourceUri array", { mcpAppResourceUri: ["FB-URI-ARR"] }, "FB-URI-ARR"],
  ["appContext.actionName object", { appContext: { connectorId: "c-1", actionName: { s: "FB-ACTION-OBJ" } } }, "FB-ACTION-OBJ"],
  ["appContext.resourceUri array", { appContext: { connectorId: "c-1", resourceUri: ["FB-RES-ARR"] } }, "FB-RES-ARR"],
  ["error.message object", { error: { message: { s: "FB-ERRMSG-OBJ" } } }, "FB-ERRMSG-OBJ"],
]) {
  test(`F-B producer: a non-scalar at ${label} is refused and never retained`, () => {
    const projected = projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "install_surface_allowed_probe" },
          result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
          ...patch,
        },
      },
    });
    assert.equal(
      JSON.stringify(projected).includes(marker),
      false,
      `${label}: host content must not reach retained bytes even though the consumer would refuse the record`,
    );
    assert.equal(
      classifyStoredEvent(projected).type,
      "unclassified",
      `${label}: the record must still be refused`,
    );
  });
}

// --- The CONSUMER allowedKeys list, pinned in both directions -------------------------------
// There are two whitelists: this one, in retainedItemReason, and a separate list in
// projectRetainedItem. Every earlier schema test asserted on __unexpectedKeys, which the PRODUCER
// list makes -- so the consumer list was pinned in neither direction, and dropping a declared
// property from it made a driven run fail all nine cells while the suite stayed green.
//
// The two directions need two different tests, and one cannot substitute for the other:
//   * dropping a declared key is caught by a well-formed record being ACCEPTED;
//   * adding an undeclared key is NOT caught that way, because a well-formed record never carries
//     the added key. It is also not caught through the projection, because the producer still marks
//     the key and the consumer then refuses on the __unexpectedKeys marker rather than on the key
//     itself. Only an UNPROJECTED stored event isolates the consumer list.

test("consumer accepts a well-formed mcpToolCall carrying every newly admitted field", () => {
  const classified = classifyStoredEvent(
    projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "install_surface_allowed_probe" },
          result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
          durationMs: 120,
          readOnlyHint: true,
          pluginId: "plugin-assay",
          mcpAppResourceUri: "resource://assay",
          appContext: { connectorId: "connector-1" },
          error: null,
        },
      },
    }),
  );
  assert.equal(
    classified.type,
    "server-notification",
    `a record carrying only declared 0.153.1 properties must be accepted; a consumer allowedKeys missing any of them silently refuses every real host record (got: ${JSON.stringify(classified)})`,
  );
});

test("consumer refuses an unprojected mcpToolCall carrying a name the schema does not declare", () => {
  // Deliberately NOT projected: the producer would mark the key and the consumer would then refuse
  // on the marker, which proves nothing about the consumer's own list. These stored bytes isolate it.
  //
  // SCOPE: a sample, not a proof of set equality. It refuses the names below; it cannot establish
  // that every name outside the schema is refused, because it does not enumerate that set.
  for (const key of [
    "notInTheSchemaAtAll",
    "aggregatedOutput",
    "command",
    "cwd",
    "sessionId",
  ]) {
    const classified = classifyStoredEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "install_surface_allowed_probe" },
          result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
          [key]: null,
        },
      },
    });
    assert.equal(
      classified.type,
      "unclassified",
      `${key} is not declared by the 0.153.1 schema; the consumer must refuse it on its own list`,
    );
  }
});

// --- F-B, extended: durationMs and readOnlyHint have their own producer ternaries ------------
// They do not share projectedPresence, so the five F-B rows above do not cover them, and the
// existing cases use scalar non-strings which the consumer catches regardless of the producer.
for (const [label, patch, marker] of [
  ["durationMs object", { durationMs: { secret: "FB-DURATION-OBJ" } }, "FB-DURATION-OBJ"],
  ["readOnlyHint array", { readOnlyHint: ["FB-READONLY-ARR"] }, "FB-READONLY-ARR"],
]) {
  test(`F-B producer: a non-scalar at ${label} is refused and never retained`, () => {
    const projected = projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "install_surface_allowed_probe" },
          result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
          ...patch,
        },
      },
    });
    assert.equal(JSON.stringify(projected).includes(marker), false, `${label}: must not reach retained bytes`);
    assert.equal(classifyStoredEvent(projected).type, "unclassified", `${label}: must still be refused`);
  });
}

// --- F-1: an object KEY NAME is host data exactly as a value is ----------------------------
// Every earlier privacy test scanned VALUE positions. The unexpected-key marker recorded the
// NAMES of the keys it rejected, so a host that put a path or a token in a key name had it
// retained in full -- and at three of the six sites the record was still classified accepted,
// so the leak was not even redeemed by refusal. The projection now records only that unexpected
// keys existed. These tests assert the name is gone at every site that can produce the marker.
const KEY_NAME_PROBE = "/Users/alice/.ssh/id_rsa?token=AKIA_KEY_NAME_PROBE";

function everyKey(value, seen = new Set()) {
  if (Array.isArray(value)) {
    for (const entry of value) everyKey(entry, seen);
  } else if (value !== null && typeof value === "object") {
    for (const [key, entry] of Object.entries(value)) {
      seen.add(key);
      everyKey(entry, seen);
    }
  }
  return seen;
}

for (const [site, item] of [
  ["projectRetainedItem", { [KEY_NAME_PROBE]: 1 }],
  ["projectArguments", { arguments: { tool: "probe", [KEY_NAME_PROBE]: 1 } }],
  ["projectToolResult", { result: { isError: false, [KEY_NAME_PROBE]: 1 } }],
  [
    "projectDecisionObject",
    { result: { isError: false, structuredContent: { allowed: true, reason: "ok", [KEY_NAME_PROBE]: 1 } } },
  ],
  ["projectAppContext", { appContext: { connectorId: "c-1", [KEY_NAME_PROBE]: 1 } }],
  ["projectMcpToolCallError", { error: { message: "boom", [KEY_NAME_PROBE]: 1 } }],
]) {
  test(`F1: a secret-shaped key name at ${site} is marked without being retained`, () => {
    const projected = projectRetainedEvent({
      direction: "server",
      method: "item/completed",
      id: null,
      params: {
        completedAtMs: 5000,
        threadId: "t-1",
        turnId: "u-1",
        item: {
          type: "mcpToolCall",
          id: "tool-1",
          server: "assay",
          tool: "assay_policy_decide",
          status: "completed",
          arguments: { tool: "probe" },
          result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
          ...item,
        },
      },
    });
    assert.equal(
      JSON.stringify(projected).includes(KEY_NAME_PROBE),
      false,
      `${site}: the key name must not reach retained bytes`,
    );
    assert.equal(
      everyKey(projected).has(KEY_NAME_PROBE),
      false,
      `${site}: the key name must not survive as a key either`,
    );
    // Positive control: without this the assertions above pass on a projection that never
    // reached the marker at all, and the test would prove nothing about this site.
    assert.equal(
      everyKey(projected).has("__unexpectedKeys"),
      true,
      `${site}: the marker must still record that an unexpected key was present`,
    );
  });
}

test("F1: the unexpected-key marker is a re-projection fixed point", () => {
  // The marker is itself outside every allowed list, so it is re-marked on a second pass. That is
  // only stable because the marker collapses to a constant: a name list or a count would change.
  const event = {
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: {
        type: "mcpToolCall",
        id: "tool-1",
        server: "assay",
        tool: "assay_policy_decide",
        status: "completed",
        arguments: { tool: "probe", [KEY_NAME_PROBE]: 1 },
        [KEY_NAME_PROBE]: 1,
        appContext: { connectorId: "c-1", [KEY_NAME_PROBE]: 1 },
        result: {
          isError: false,
          [KEY_NAME_PROBE]: 1,
          structuredContent: { allowed: true, reason: "ok", [KEY_NAME_PROBE]: 1 },
        },
      },
    },
  };
  const once = projectRetainedEvent(event);
  const twice = projectRetainedEvent(once);
  // SCOPE: this establishes the fixed point for the UNEXPECTED-KEY MARKER only. It is not a claim
  // that projection is idempotent over all events -- `__invalidType` markers are deliberately not
  // fixed points, and re-project to a different marker. That is fail-closed and out of scope here.
  assert.deepEqual(twice, once, "the unexpected-key marker must be a fixed point");
  assert.equal(JSON.stringify(once).includes(KEY_NAME_PROBE), false);
});

test("F1: a secret-shaped key name never reaches the persisted events.json", async () => {
  // The projection tests above run in process. This one drives the real fake host through the
  // real driver and asserts against the bytes on disk, because events.json is written
  // unconditionally before any verdict is reached.
  const { proofRoot, events } = await drive("host-key-name-leak");
  const raw = fs.readFileSync(path.join(proofRoot, "events.json"), "utf8");
  assert.equal(
    raw.includes("AKIA_KEY_NAME_PROBE"),
    false,
    "the persisted proof must not contain the host-controlled key name",
  );
  assert.equal(
    fs.readFileSync(path.join(proofRoot, "classification.json"), "utf8").includes("AKIA_KEY_NAME_PROBE"),
    false,
    "the refusal reason must not echo the host-controlled key name",
  );
  // Positive control: the scenario must actually have reached the pipeline. Without this the
  // absence assertions pass vacuously if the fixture literal ever drifts.
  const toolEvent = events.find(
    (event) => event.method === "item/completed" && event.params?.item?.type === "mcpToolCall",
  );
  assert.ok(toolEvent, "the scenario must emit an mcpToolCall item");
  assert.equal(
    toolEvent.params.item.__unexpectedKeys,
    "[present]",
    "control: the injected key must have been marked, or nothing was measured",
  );
});

// --- F1/F3: an unknown method name and an unknown item type are host text too ------------------
// The key-name repair closed one identifier position. These are the other two the independent
// review found: a JSON-RPC method and a retained item type, both host-controlled strings that were
// retained verbatim. Known names must survive verbatim -- classification, correlation and the cells
// are keyed on them -- so the projection is a closed-set check, not a scrub.
const IDENTIFIER_PROBE = "/Users/alice/.aws/credentials?sk=IDENTIFIER_PROBE";

test("F1: an unknown method is projected, and every declared method survives verbatim", () => {
  const projected = projectRetainedEvent({
    direction: "server",
    method: `notify_${IDENTIFIER_PROBE}`,
    id: null,
    params: {},
  });
  // Explicitly invalid, not a legal-looking substitute: an unknown member must never read as an
  // allowed value, and the consumer refuses the violation wherever it sits.
  assert.equal(typeof projected.method, "object", "an unknown method must become a violation");
  assert.equal(JSON.stringify(projected).includes(IDENTIFIER_PROBE), false);
  assert.equal(
    classifyStoredEvent(projected).type,
    "unclassified",
    "an unknown method must still be refused, not merely projected",
  );
  // Positive control: every method this proof declares must pass through untouched, or the
  // projection above is a scrub that would break classification rather than a closed-set check.
  for (const method of KNOWN_METHODS) {
    assert.equal(
      projectRetainedEvent({ direction: "server", method, id: null, params: {} }).method,
      method,
      `declared method ${method} must survive verbatim`,
    );
  }
});

test("F3: an unknown item type is projected, and every declared type survives verbatim", () => {
  const projectedType = projectRetainedItem_typeOf(`type_${IDENTIFIER_PROBE}`);
  assert.equal(typeof projectedType, "object", "an unknown item type must become a violation");
  assert.equal(JSON.stringify(projectedType).includes(IDENTIFIER_PROBE), false);
  for (const type of KNOWN_ITEM_TYPES) {
    assert.equal(projectRetainedItem_typeOf(type), type, `declared type ${type} must survive`);
  }
});

test("F1/F3: neither identifier reaches the persisted proof", async () => {
  const { proofRoot, events } = await drive("host-identifier-leak");
  for (const name of ["events.json", "classification.json", "manifest.json"]) {
    assert.equal(
      fs.readFileSync(path.join(proofRoot, name), "utf8").includes("IDENTIFIER_PROBE"),
      false,
      `${name} must not contain the host-controlled identifier`,
    );
  }
  // Positive control: the scenario must have reached the pipeline, or the absence is vacuous.
  // Positive controls: the scenario must have reached the pipeline. Both identifiers must appear
  // as explicit violations, or the absence assertions above are vacuous.
  assert.ok(
    events.some((event) => event.method !== null && typeof event.method === "object"),
    "control: the unknown method must have been projected to a violation",
  );
  assert.ok(
    events.some((event) => typeof event.params?.item?.type === "object"),
    "control: the unknown item type must have been projected to a violation",
  );
});

// --- F4: a projection violation is a violation wherever it sits -------------------------------
test("F4: a marker nested under result is refused, not carried into a green proof", () => {
  const projected = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: {
        type: "mcpToolCall",
        id: "tool-1",
        server: "assay",
        tool: "assay_policy_decide",
        status: "completed",
        arguments: { tool: "probe" },
        result: { isError: false, structuredContent: { allowed: true, reason: "ok" }, extra: 1 },
      },
    },
  });
  // The producer marks it; before this repair the consumer consulted the marker only at some
  // levels, so this record passed every cell -- a fully green proof carrying a recorded violation.
  assert.equal(projected.params.item.result.__unexpectedKeys, "[present]");
  const classified = classifyStoredEvent(projected);
  assert.equal(classified.type, "unclassified", "a nested marker must refuse");
  assert.match(classified.reason, /projection violation/);
  // Positive control: the same record without the extra key must still be accepted, or the check
  // above is satisfied by refusing everything.
  const clean = projectRetainedEvent({
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: {
        type: "mcpToolCall",
        id: "tool-1",
        server: "assay",
        tool: "assay_policy_decide",
        status: "completed",
        arguments: { tool: "probe" },
        result: { isError: false, structuredContent: { allowed: true, reason: "ok" } },
      },
    },
  });
  assert.equal(classifyStoredEvent(clean).type, "server-notification", "control: clean must pass");
});

// --- F2: a host-chosen string rpc id is host text, and correlation is only equality -----------
// The wire keeps the real id -- string ids stay fully supported on the protocol. Only what is
// RETAINED is relabelled, by one per-run first-seen ordinal map at the single retention funnel.
// These tests pin both halves: the id never reaches the proof, and the correlation it carries does.
const RPC_ID_PROBE = "RPCID_PROBE";

test("F2: a host-chosen string rpc id never reaches the persisted proof", async () => {
  const { proofRoot, events, classified } = await drive("host-rpc-id-leak");
  for (const name of ["events.json", "classification.json", "manifest.json"]) {
    assert.equal(
      fs.readFileSync(path.join(proofRoot, name), "utf8").includes(RPC_ID_PROBE),
      false,
      `${name} must not contain the host-chosen rpc id`,
    );
  }
  // Positive control: the scenario must have reached the pipeline, or every absence above is
  // vacuous. A projected token must be present where the host id was.
  const serverRequest = events.find(
    (event) => event.direction === "server" && event.method === "mcpServer/elicitation/request",
  );
  assert.ok(serverRequest, "control: the server request must have been retained");
  assert.match(
    String(serverRequest.id),
    /^#\d+$/,
    "control: the host id must have been projected to a token",
  );
  // The property the token exists for: the matching client response must still pair with it.
  const clientResponse = events.find(
    (event) => event.direction === "client" && event.method === "mcpServer/elicitation/request",
  );
  assert.ok(clientResponse, "the matching client response must be retained");
  assert.equal(
    clientResponse.id,
    serverRequest.id,
    "request and response must still correlate through the token",
  );
  // The strongest available control: with a secret-shaped host id, the proof must still pass every
  // intended cell. If the token had broken correlation anywhere, this is where it would show.
  assert.equal(
    allIntendedCellsPass(classified),
    true,
    "relabelling the retained id must not cost the proof a single cell",
  );
});

test("F2: the retained token preserves equality structure and leaves integers alone", async () => {
  const { events } = await drive("host-rpc-id-leak");
  const stringIds = events.filter((event) => typeof event.id === "string").map((e) => e.id);
  const numericIds = events.filter((event) => Number.isSafeInteger(event.id)).map((e) => e.id);

  // Every retained string id is a token; none is a raw host string.
  for (const id of stringIds) {
    assert.match(id, /^#\d+$/, `retained string id ${id} must be a token`);
  }
  // Repeated: the request and its response share one host id, so they share one token.
  assert.ok(
    stringIds.length > new Set(stringIds).size,
    "a repeated host id must yield a repeated token, or correlation is not being exercised",
  );
  // NOTE: injectivity is deliberately NOT asserted here. This run carries one host id, so nothing
  // in it could distinguish an injective map from one that collapses every id to a single token.
  // The separate distinct-id test below is the only place that discriminates it.

  // Numeric ids are driver-generated and pass through untouched, which is what keeps the
  // client-generated integer response lookup working.
  assert.ok(numericIds.length > 0, "the driver's integer request ids must still be retained");
  for (const id of numericIds) {
    assert.equal(typeof id, "number");
  }
});

test("F2: distinct host ids get distinct tokens", async () => {
  // The discriminating case. A map that returned one constant token would satisfy every other F2
  // assertion -- absence, tokenisation, and even request/response pairing -- so without two
  // distinct host ids in one run, injectivity is asserted but untested.
  const { proofRoot, events } = await drive("host-rpc-id-distinct");
  const raw = fs.readFileSync(path.join(proofRoot, "events.json"), "utf8");
  for (const probe of ["RPCID_PROBE", "RPCID_PROBE_TWO"]) {
    assert.equal(raw.includes(probe), false, `${probe} must not reach the persisted proof`);
  }
  const requestTokens = events
    .filter(
      (event) =>
        event.direction === "server" && event.method === "mcpServer/elicitation/request",
    )
    .map((event) => event.id);
  assert.equal(requestTokens.length, 2, "control: both server requests must have been retained");
  for (const token of requestTokens) {
    assert.match(String(token), /^#\d+$/);
  }
  assert.notEqual(
    requestTokens[0],
    requestTokens[1],
    "two different host ids must not collapse to one token",
  );
});

// --- B/C/D: the finite identifier contract, measured on the persisted bytes ------------------
// Every assertion below reads events.json and classification.json, not an in-process projection:
// those files are what a live proof hands to a reader.

function persisted(proofRoot, name) {
  return fs.readFileSync(path.join(proofRoot, name), "utf8");
}

test("B: thread, turn and item identities are retained as structure, not as host text", async () => {
  const { proofRoot } = await drive("valid");
  const raw = persisted(proofRoot, "events.json");
  const events = JSON.parse(raw);

  // No host-chosen identifier text survives. The fixture's own literals are the probe.
  for (const hostValue of ["thread-", "turn-1", "call-1"]) {
    assert.equal(
      raw.includes(hostValue),
      false,
      `${hostValue} is a host-chosen identifier and must not reach the persisted proof`,
    );
  }
  assert.equal(
    persisted(proofRoot, "classification.json").includes("turn-1"),
    false,
    "classification must not carry a host identifier either",
  );

  // Namespaces stay separate: a thread token can never equal a turn or item token.
  const threadTokens = new Set();
  const turnTokens = new Set();
  const itemTokens = new Set();
  for (const event of events) {
    const p = event.params ?? {};
    if (typeof p.threadId === "string") threadTokens.add(p.threadId);
    if (typeof p.turnId === "string") turnTokens.add(p.turnId);
    if (typeof p.turn?.id === "string") turnTokens.add(p.turn.id);
    if (typeof p.item?.id === "string") itemTokens.add(p.item.id);
    if (typeof event.result?.thread?.id === "string") threadTokens.add(event.result.thread.id);
    if (typeof event.result?.turn?.id === "string") turnTokens.add(event.result.turn.id);
  }
  assert.ok(threadTokens.size > 0 && turnTokens.size > 0 && itemTokens.size > 0, "control: all three roles must appear");
  for (const [label, tokens, prefix] of [
    ["thread", threadTokens, "@"],
    ["turn", turnTokens, "~"],
    ["item", itemTokens, "!"],
  ]) {
    for (const value of tokens) {
      assert.ok(value.startsWith(prefix), `${label} token ${value} must carry its own namespace`);
    }
  }
  // The distinctions the cells depend on: a role's members must agree with each other, which is
  // exactly what a partial namespace would break.
  // Tokens within a role are allocated first-seen and contiguous, which is what makes them
  // injective. (This journey deliberately touches several threads, so the count is not 1.)
  for (const [label, tokens, prefix] of [
    ["thread", threadTokens, "@"],
    ["turn", turnTokens, "~"],
    ["item", itemTokens, "!"],
  ]) {
    const ordinals = [...tokens].map((value) => Number(value.slice(prefix.length))).sort((a, b) => a - b);
    assert.deepEqual(
      ordinals,
      ordinals.map((_, index) => index),
      `${label} tokens must be contiguous first-seen ordinals`,
    );
  }
  // Correlation, which is the only reason the tokens exist: the terminal turn must still name the
  // same turn the turn/start response reported.
  const terminal = events.find((event) => event.method === "turn/completed");
  const turnStart = events.find((event) => typeof event.result?.turn?.id === "string");
  assert.ok(terminal && turnStart, "control: both turn rows must be retained");
  assert.equal(
    terminal.params.turn.id,
    turnStart.result.turn.id,
    "the terminal turn must still correlate with the turn/start response",
  );
});

test("C: host platform and skill scope are retained as presence only", async () => {
  const { proofRoot, manifest } = await drive("valid");
  assert.equal(manifest.initialize.platformFamily, "[present]");
  assert.equal(manifest.initialize.platformOs, "[present]");
  // Control: a field with a real lexical consumer must NOT be collapsed, or the projection is a
  // blanket scrub rather than the finite contract.
  assert.notEqual(
    manifest.initialize.codexHome,
    "[present]",
    "codexHome has a lexical consumer and must stay verbatim",
  );
  assert.equal(persisted(proofRoot, "events.json").includes(os.platform()), false);
});

test("D: the project-root comparison is retained, and its outcome is not the path", async () => {
  const good = await drive("valid");
  const goodEvents = JSON.parse(persisted(good.proofRoot, "events.json"));
  const goodStart = goodEvents.find((event) => event.result?.cwd !== undefined);
  assert.ok(goodStart, "control: a thread/start result must be retained");
  assert.equal(goodStart.result.cwd, "[project-root]");
  // NOTE, and it is a real distinction: the project root DOES still appear in retained bytes, in
  // client REQUEST params the driver itself sent (`thread/start.params.cwd`, `skills/list.cwds`).
  // That is the driver disclosing its own value, not host content, and group D never claimed to
  // remove it. What must not survive is the value the HOST reported back, so that is what is
  // asserted -- on the result position only.
  assert.equal(
    JSON.stringify(goodStart.result).includes(good.projectRoot),
    false,
    "the host-reported cwd must not be retained in the result",
  );

  // The failing path is where a leak would matter most, so it is measured, not assumed.
  const bad = await drive("wrong-cwd");
  const badRaw = persisted(bad.proofRoot, "events.json");
  const badEvents = JSON.parse(badRaw);
  const badStart = badEvents.find((event) => event.result?.cwd !== undefined);
  assert.ok(badStart, "a thread/start result must be retained on the failing path too");
  assert.equal(badStart.result.cwd, "[not-project-root]");
  assert.equal(badRaw.includes("/not/the/project"), false, "the host cwd must not be retained");
  const badClassification = persisted(bad.proofRoot, "classification.json");
  assert.equal(
    badClassification.includes("/not/the/project"),
    false,
    "the refusal reason must not echo the host cwd",
  );
  const cells = JSON.parse(badClassification).cells;
  assert.equal(cells.cwdObserved.status, "fail", "required/false must stay distinguished from missing");
  assert.match(cells.cwdObserved.reason, /not the project root/);
});

// --- #2807: the live 0.153.1 item result carries `_meta`, and it is schema-declared -----------
// Measured against the pinned host schema
// (openai/codex rust-v0.153.1, codex-rs/app-server-protocol/src/protocol/v2/mcp.rs):
//
//   pub struct McpToolCallResult {
//       pub content: Vec<JsonValue>,
//       pub structured_content: Option<JsonValue>,
//       #[serde(rename = "_meta")] pub meta: Option<JsonValue>,
//   }
//
// No field carries skip_serializing_if, so `structuredContent: null` AND `_meta: null` are both
// always serialized. This is NOT CallToolResult and NOT McpServerToolCallResponse -- those declare
// isError and DO skip None. The projection was missing `_meta`, so a schema-correct live record was
// marked unexpected and the consumer refused it. The refusal was correct behaviour on an incomplete
// allowed-field list, not a host fault.
const LIVE_0153_RESULT = Object.freeze({
  content: [{ type: "text", text: JSON.stringify({ allowed: true, reason: "Allowed by policy" }) }],
  structuredContent: null,
  _meta: null,
});

function liveToolEvent(result) {
  return {
    direction: "server",
    method: "item/completed",
    id: null,
    params: {
      completedAtMs: 5000,
      threadId: "t-1",
      turnId: "u-1",
      item: {
        type: "mcpToolCall",
        id: "tool-1",
        server: "assay",
        tool: "assay_policy_decide",
        status: "completed",
        arguments: { tool: "install_surface_allowed_probe" },
        result,
      },
    },
  };
}

test("#2807: a schema-shaped 0.153.1 result with _meta:null is accepted, not marked unexpected", () => {
  const projected = projectRetainedEvent(liveToolEvent(LIVE_0153_RESULT));
  assert.equal(
    Object.hasOwn(projected.params.item.result, "__unexpectedKeys"),
    false,
    "_meta is a declared field of McpToolCallResult and must not be marked unexpected",
  );
  assert.equal(
    classifyStoredEvent(projected).type,
    "server-notification",
    "the live host record must be accepted by the consumer",
  );
});

test("#2807: _meta is retained as presence only, never as metadata content", () => {
  const projected = projectRetainedEvent(
    liveToolEvent({
      ...LIVE_0153_RESULT,
      _meta: { traceId: "/Users/alice/.ssh/id_rsa?tok=META_SECRET_CANARY", depth: 3 },
    }),
  );
  const raw = JSON.stringify(projected);
  assert.equal(raw.includes("META_SECRET_CANARY"), false, "a metadata value must not be retained");
  assert.equal(raw.includes("traceId"), false, "a metadata key NAME must not be retained either");
  assert.equal(
    projected.params.item.result._meta,
    "[present]",
    "a non-null _meta is recorded as presence",
  );
});

test("#2807: null and present _meta stay distinguishable, and absent stays absent", () => {
  const withNull = projectRetainedEvent(liveToolEvent(LIVE_0153_RESULT)).params.item.result;
  const withValue = projectRetainedEvent(
    liveToolEvent({ ...LIVE_0153_RESULT, _meta: { a: 1 } }),
  ).params.item.result;
  const { _meta, ...withoutMeta } = LIVE_0153_RESULT;
  const absent = projectRetainedEvent(liveToolEvent(withoutMeta)).params.item.result;
  assert.equal(withNull._meta, null, "a declared null must stay null, not become presence");
  assert.equal(withValue._meta, "[present]");
  assert.equal(Object.hasOwn(absent, "_meta"), false, "an absent field must stay absent");
});

test("#2807: admitting _meta does not admit an undeclared sibling", () => {
  const projected = projectRetainedEvent(
    liveToolEvent({ ...LIVE_0153_RESULT, hostExtension: { anything: 1 } }),
  );
  assert.equal(
    projected.params.item.result.__unexpectedKeys,
    "[present]",
    "an undeclared sibling must still be marked",
  );
  assert.equal(
    classifyStoredEvent(projected).type,
    "unclassified",
    "and the consumer must still refuse it -- this is not a blanket unknown-key allowance",
  );
});

test("#2807: both _meta branches stay re-projection fixed points", () => {
  // The first version of this test used `_meta: null` only, so it never re-projected the PRESENT
  // branch whose idempotence its own comment asserted -- a non-idempotent non-null branch survived
  // it at 171/171. Both branches are exercised, because the null branch cannot discriminate the
  // one that matters.
  for (const [label, result] of [
    ["null _meta", LIVE_0153_RESULT],
    ["present _meta", { ...LIVE_0153_RESULT, _meta: { traceId: "x", nested: { deep: [1, 2] } } }],
  ]) {
    const once = projectRetainedEvent(liveToolEvent(result));
    assert.deepEqual(projectRetainedEvent(once), once, `${label} must be a fixed point`);
  }
  // Control: the present branch must actually have reached PRESENT, or the loop above proved
  // nothing about it.
  assert.equal(
    projectRetainedEvent(liveToolEvent({ ...LIVE_0153_RESULT, _meta: { a: 1 } })).params.item.result
      ._meta,
    "[present]",
  );
});

test("#2807: admitting _meta still carries error and isError to the consumer", () => {
  // LAYER, stated because I first asserted the wrong one: `classifyStoredEvent` judges event
  // SHAPE, and it accepts these -- the refusal for `isError: true` and for a non-Boolean lives in
  // `classifyToolResultEnvelope`, reached through `classifyRecord` as a cell judgement, and the
  // existing driven tests pin it there. What THIS layer can establish, and what the retention of
  // these fields is actually for, is that the projection carries them through at all: a field the
  // producer dropped could never reach the check that refuses it.
  const withIsError = projectRetainedEvent(
    liveToolEvent({ ...LIVE_0153_RESULT, isError: true }),
  ).params.item.result;
  assert.equal(withIsError.isError, true, "isError must survive projection to reach its refusal");
  assert.equal(
    Object.hasOwn(withIsError, "__unexpectedKeys"),
    false,
    "isError is an allowed field, not an unexpected one",
  );

  const withError = projectRetainedEvent(
    liveToolEvent({ ...LIVE_0153_RESULT, error: { message: "tool exploded" } }),
  ).params.item.result;
  assert.deepEqual(withError.error, { present: true }, "error survives as presence, not content");
  assert.equal(
    JSON.stringify(withError).includes("tool exploded"),
    false,
    "an error message must not be retained",
  );
});

test("#2807: the producer's isError type barrier is the deciding check for a non-Boolean", () => {
  // Pins the PRODUCER. Deleting the `typeof === "boolean"` conversion in projectToolResult
  // previously changed nothing observable, because the consumer refuses a non-Boolean
  // independently -- the same shape as the F-B durationMs/readOnlyHint gap. Asserting the
  // projected bytes makes the barrier the deciding check.
  assert.equal(
    projectRetainedEvent(liveToolEvent({ ...LIVE_0153_RESULT, isError: "true" })).params.item.result
      .isError,
    "[invalid]",
    "a non-Boolean isError must be converted by the producer, not carried verbatim",
  );
  // Control: a real Boolean must survive unchanged, or the conversion is a blanket rewrite.
  assert.equal(
    projectRetainedEvent(liveToolEvent({ ...LIVE_0153_RESULT, isError: false })).params.item.result
      .isError,
    false,
  );
});

test("#2812: live-shaped terminal turn carrying itemsView: 'summary' omits tool item but passes oneToolInvoked", async () => {
  const control = await drive("valid");
  const events = structuredClone(control.events);
  const terminal = events.find((e) => e.method === "turn/completed");
  assert.ok(terminal, "control events must contain a matching turn/completed");

  // In live Codex 0.153.1 captures, the streamed tool item is delivered via item/completed,
  // while the terminal turn carries itemsView: "summary" and only contains the display
  // agentMessage (omitting the tool item from turn.items).
  terminal.params.turn.itemsView = "summary";
  terminal.params.turn.items = [
    {
      id: "!5",
      type: "agentMessage",
    },
  ];

  const classified = classifyRecord({ ...control.manifest, events });
  assert.equal(
    classified.cells.oneToolInvoked.status,
    "pass",
    "summary view omitting tool item must pass oneToolInvoked when tool was canonically streamed",
  );
  assert.equal(
    classified.cells.structuredResultValidated.status,
    "pass",
    "structuredResultValidated must pass when summary invocation passes",
  );
});

test("#2812: driven test passes end-to-end with terminal-summary scenario", async () => {
  const result = await drive("terminal-summary");
  assert.equal(result.childExitCode, 0);
  assert.equal(result.driverOutcome.exitCode, 0);
  assert.equal(result.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(result.classified.cells.structuredResultValidated.status, "pass");
  assert.equal(allIntendedCellsPass(result.classified), true);

  const terminal = result.events.find((e) => e.method === "turn/completed");
  assert.ok(terminal);
  assert.equal(terminal.params.turn.itemsView, "summary");
});

test("#2812: driven test passes end-to-end with terminal-not-loaded scenario", async () => {
  const result = await drive("terminal-not-loaded");
  assert.equal(result.childExitCode, 0);
  assert.equal(result.driverOutcome.exitCode, 0);
  assert.equal(result.classified.cells.oneToolInvoked.status, "pass");
  assert.equal(result.classified.cells.structuredResultValidated.status, "pass");
  assert.equal(allIntendedCellsPass(result.classified), true);

  const terminal = result.events.find((e) => e.method === "turn/completed");
  assert.ok(terminal);
  assert.equal(terminal.params.turn.itemsView, "notLoaded");
});

test("#2812: summary view behavioral matrix (omitted tool, matching tool, conflicting tool, duplicate tools)", async () => {
  const control = await drive("valid");
  const canonicalTool = control.events.find(
    (e) => e.method === "item/completed" && e.params?.item?.type === "mcpToolCall",
  ).params.item;

  // Case 1: summary with matching tool item
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "summary";
    terminal.params.turn.items = [
      { id: "!5", type: "agentMessage" },
      structuredClone(canonicalTool),
    ];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "pass");
  }

  // Case 2: summary with conflicting tool item (contradicts canonical)
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "summary";
    const mutatedTool = structuredClone(canonicalTool);
    mutatedTool.arguments = { tool: "wrong_probe", policy: "wrong.yaml" };
    terminal.params.turn.items = [mutatedTool];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal tool item contradicts item/completed",
    );
  }

  // Case 3: summary with tool item carrying different id
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "summary";
    const mutatedTool = structuredClone(canonicalTool);
    mutatedTool.id = "different-id";
    terminal.params.turn.items = [mutatedTool];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal must contain exactly the canonical tool item",
    );
  }

  // Case 4: summary with duplicate tool items
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "summary";
    terminal.params.turn.items = [
      structuredClone(canonicalTool),
      structuredClone(canonicalTool),
    ];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal contains duplicate tool items",
    );
  }
});

test("#2812: full view behavioral matrix (matching tool required, missing tool, conflicting tool, duplicate tools)", async () => {
  const control = await drive("valid");
  const canonicalTool = control.events.find(
    (e) => e.method === "item/completed" && e.params?.item?.type === "mcpToolCall",
  ).params.item;

  // Case 1: full view with matching tool item
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "full";
    terminal.params.turn.items = [structuredClone(canonicalTool)];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "pass");
  }

  // Case 2: full view missing tool item (e.g. only agentMessage)
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "full";
    terminal.params.turn.items = [{ id: "!5", type: "agentMessage" }];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal must contain exactly the canonical tool item",
    );
  }

  // Case 3: full view with conflicting tool item
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "full";
    const mutatedTool = structuredClone(canonicalTool);
    mutatedTool.arguments = { tool: "wrong_probe", policy: "wrong.yaml" };
    terminal.params.turn.items = [mutatedTool];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal tool item contradicts item/completed",
    );
  }

  // Case 4: full view with duplicate tool items
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "full";
    terminal.params.turn.items = [
      structuredClone(canonicalTool),
      structuredClone(canonicalTool),
    ];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal contains duplicate tool items",
    );
  }
});

test("#2812: notLoaded view behavioral matrix (empty items required, non-empty items rejected)", async () => {
  const control = await drive("valid");
  const canonicalTool = control.events.find(
    (e) => e.method === "item/completed" && e.params?.item?.type === "mcpToolCall",
  ).params.item;

  // Case 1: notLoaded with empty items
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "notLoaded";
    terminal.params.turn.items = [];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "pass");
  }

  // Case 2: notLoaded with non-tool item (agentMessage)
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "notLoaded";
    terminal.params.turn.items = [{ id: "!5", type: "agentMessage" }];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "notLoaded turn items must be empty",
    );
  }

  // Case 3: notLoaded with tool item
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = "notLoaded";
    terminal.params.turn.items = [structuredClone(canonicalTool)];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "notLoaded turn items must be empty",
    );
  }
});

test("#2812: absent itemsView preserves exact legacy requirement (matching tool required)", async () => {
  const control = await drive("valid");
  const canonicalTool = control.events.find(
    (e) => e.method === "item/completed" && e.params?.item?.type === "mcpToolCall",
  ).params.item;

  // Case 1: absent itemsView with matching tool item passes
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    delete terminal.params.turn.itemsView;
    terminal.params.turn.items = [structuredClone(canonicalTool)];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "pass");
  }

  // Case 2: absent itemsView missing tool item fails
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    delete terminal.params.turn.itemsView;
    terminal.params.turn.items = [{ id: "!5", type: "agentMessage" }];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal must contain exactly the canonical tool item",
    );
  }

  // Case 3: absent itemsView with conflicting tool item fails
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    delete terminal.params.turn.itemsView;
    const mutatedTool = structuredClone(canonicalTool);
    mutatedTool.arguments = { tool: "wrong_probe", policy: "wrong.yaml" };
    terminal.params.turn.items = [mutatedTool];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal tool item contradicts item/completed",
    );
  }

  // Case 4: absent itemsView with duplicate tool items fails
  {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    delete terminal.params.turn.itemsView;
    terminal.params.turn.items = [
      structuredClone(canonicalTool),
      structuredClone(canonicalTool),
    ];
    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(classified.cells.oneToolInvoked.status, "fail");
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "matching terminal contains duplicate tool items",
    );
  }
});

test("#2812: unknown, null, and non-string itemsView are refused closed without broad acceptance", async () => {
  const control = await drive("valid");
  const badViews = ["unknown_view", null, 123, "", {}, false, ["summary"]];

  for (const badView of badViews) {
    const events = structuredClone(control.events);
    const terminal = events.find((e) => e.method === "turn/completed");
    terminal.params.turn.itemsView = badView;

    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(
      classified.cells.oneToolInvoked.status,
      "fail",
      `bad itemsView ${JSON.stringify(badView)} must fail oneToolInvoked`,
    );
    assert.equal(
      classified.cells.oneToolInvoked.reason,
      "journey topology: turn/completed itemsView must be full, summary, or notLoaded",
    );

    const eventCheck = classifyStoredEvent(terminal);
    assert.equal(
      eventCheck.type,
      "unclassified",
      `bad itemsView ${JSON.stringify(badView)} must yield unclassified event`,
    );
    assert.equal(
      eventCheck.reason,
      "turn/completed itemsView must be full, summary, or notLoaded",
    );
  }
});

test("#2812: Refinement 1: hostile itemsView canary never leaks into retained reasons or projected events", () => {
  const CANARY = "LEAK_SECRET_ITEMS_VIEW_987654";
  const rawTerminalEvent = {
    direction: "server",
    id: null,
    method: "turn/completed",
    params: {
      threadId: "@0",
      turn: {
        id: "~0",
        items: [],
        itemsView: CANARY,
        status: "completed",
      },
    },
  };

  // 1. Structural reason check: must not echo hostile value
  const classified = classifyStoredEvent(rawTerminalEvent);
  assert.equal(classified.type, "unclassified");
  assert.equal(
    classified.reason.includes(CANARY),
    false,
    "retained reason must never interpolate hostile itemsView value",
  );
  assert.equal(
    classified.reason,
    "turn/completed itemsView must be full, summary, or notLoaded",
  );

  // 2. Projected event check: hostile string must be stripped to invalid projection marker
  const projected = projectRetainedEvent(rawTerminalEvent);
  const serialized = JSON.stringify(projected);
  assert.equal(
    serialized.includes(CANARY),
    false,
    "projected event must never retain hostile itemsView string",
  );
  assert.deepEqual(projected.params.turn.itemsView, { __invalidType: "string" });

  // 3. Projected event carrying violation is refused closed
  const postClassified = classifyStoredEvent(projected);
  assert.equal(postClassified.type, "unclassified");
  assert.equal(
    postClassified.reason,
    "retained event carries a projection violation",
  );
});

test("#2812: itemsView re-projection fixed-point and absent-stays-absent guarantees", () => {
  const baseTerminal = {
    direction: "server",
    id: null,
    method: "turn/completed",
    params: {
      threadId: "@0",
      turn: {
        id: "~0",
        items: [],
        status: "completed",
      },
    },
  };

  for (const view of ALLOWED_TURN_ITEMS_VIEWS) {
    const event = structuredClone(baseTerminal);
    event.params.turn.itemsView = view;
    const once = projectRetainedEvent(event);
    assert.equal(once.params.turn.itemsView, view);
    assert.deepEqual(
      projectRetainedEvent(once),
      once,
      `${view} must be a re-projection fixed point`,
    );
  }

  // Absent stays absent
  const absentEvent = structuredClone(baseTerminal);
  const projectedAbsent = projectRetainedEvent(absentEvent);
  assert.equal(
    Object.hasOwn(projectedAbsent.params.turn, "itemsView"),
    false,
    "absent itemsView must stay absent in projection",
  );
  assert.deepEqual(
    projectRetainedEvent(projectedAbsent),
    projectedAbsent,
    "absent itemsView projection must be a fixed point",
  );
});

test("#2812: Refinement 2: terminal-before-item temporal ordering refusal in event stream", async () => {
  const control = await drive("valid");
  const events = structuredClone(control.events);

  const itemIndex = events.findIndex(
    (e) => e.method === "item/completed" && e.params?.item?.type === "mcpToolCall",
  );
  const terminalIndex = events.findIndex((e) => e.method === "turn/completed");
  assert.ok(itemIndex !== -1 && terminalIndex !== -1);
  assert.ok(itemIndex < terminalIndex, "control must have item/completed before turn/completed");

  // Move turn/completed to occur BEFORE item/completed in the event stream
  const [terminal] = events.splice(terminalIndex, 1);
  events.splice(itemIndex, 0, terminal);

  const classified = classifyRecord({ ...control.manifest, events });
  assert.equal(classified.cells.oneToolInvoked.status, "fail");
  assert.equal(
    classified.cells.oneToolInvoked.reason,
    "matching turn/completed must follow item/completed in event stream",
  );

  // Control: original order passes
  const controlClassified = classifyRecord({ ...control.manifest, events: control.events });
  assert.equal(controlClassified.cells.oneToolInvoked.status, "pass");
});

test("#2812: no-item-without-stream refuses even if terminal carries a matching tool", async () => {
  const control = await drive("valid");

  for (const view of ["summary", "full", "notLoaded", undefined]) {
    const events = structuredClone(control.events).filter(
      (e) => !(e.method === "item/completed" && e.params?.item?.type === "mcpToolCall"),
    );
    const terminal = events.find((e) => e.method === "turn/completed");
    if (view !== undefined) {
      terminal.params.turn.itemsView = view;
    } else {
      delete terminal.params.turn.itemsView;
    }

    const classified = classifyRecord({ ...control.manifest, events });
    assert.equal(
      classified.cells.oneToolInvoked.status,
      "unavailable",
      `missing streamed item must make oneToolInvoked unavailable for view ${view}`,
    );
    assert.equal(classified.cells.oneToolInvoked.reason, "no mcpToolCall item/completed");
  }
});

test("#2812: failed or interrupted terminal status dominates across all itemsView variants", async () => {
  const control = await drive("valid");

  for (const view of ["summary", "full", "notLoaded", undefined]) {
    for (const badStatus of ["failed", "interrupted"]) {
      const events = structuredClone(control.events);
      const terminal = events.find((e) => e.method === "turn/completed");
      if (view !== undefined) {
        terminal.params.turn.itemsView = view;
      } else {
        delete terminal.params.turn.itemsView;
      }
      terminal.params.turn.status = badStatus;

      const classified = classifyRecord({ ...control.manifest, events });
      assert.equal(
        classified.cells.oneToolInvoked.status,
        "fail",
        `${badStatus} status must fail oneToolInvoked for view ${view}`,
      );
      assert.equal(
        classified.cells.oneToolInvoked.reason,
        `matching turn/completed status ${badStatus} is not completed`,
      );
    }
  }
});

// --- #2684 cell 1: install source is declared, bounded, and survives to disk ---

test("#2684: install source is required and never defaulted", () => {
  // Positive control first: a complete declaration is accepted.
  const ok = parseArgs([
    "--install-source", "assayMcp", "crates-io", "assay-mcp-server@6.0.0",
  ]);
  assert.deepEqual(ok.installSources.assayMcp, {
    route: "crates-io",
    reference: "assay-mcp-server@6.0.0",
  });

  // An absent declaration must fail closed rather than read as a pass.
  assert.throws(
    () => resolveHostIdentityProduction({ proofRoot: scratch(), installSources: {} }),
    /--install-source is required for: codex, codexCodeModeHost, assayMcp/,
  );
  assert.throws(
    () => resolveHostIdentityProduction({ proofRoot: scratch() }),
    /--install-source is required/,
  );
});

test("#2684: install source vocabulary is closed and references are bounded", () => {
  assert.deepEqual(
    [...INSTALL_ROUTES].sort(),
    ["crates-io", "github-release", "host-bundled", "local-build"],
  );
  // A local build is recordable but never counts as published.
  assert.equal(PUBLISHED_INSTALL_ROUTES.includes("local-build"), false);
  for (const route of PUBLISHED_INSTALL_ROUTES) {
    assert.equal(INSTALL_ROUTES.includes(route), true);
  }

  assert.equal(installSourceBound({ route: "crates-io", reference: "a@1" }), true);
  for (const [label, value] of [
    ["unknown route", { route: "sideloaded", reference: "a" }],
    ["empty reference", { route: "crates-io", reference: "" }],
    ["newline injection", { route: "crates-io", reference: "a\nVERDICT: READY" }],
    ["control character", { route: "crates-io", reference: "a\u0000b" }],
    ["credential shaped", { route: "crates-io", reference: "api_key=AKIAIOSFODNN7EXAMPLE" }],
    ["over length", { route: "crates-io", reference: "a".repeat(201) }],
    ["extra key", { route: "crates-io", reference: "a", note: "x" }],
    ["missing reference", { route: "crates-io" }],
    ["not an object", "crates-io"],
  ]) {
    assert.equal(installSourceBound(value), false, `${label} must be refused`);
  }
});

test("#2684: parseArgs refuses malformed or repeated install-source declarations", () => {
  for (const [label, argv] of [
    ["unknown subject", ["--install-source", "nope", "crates-io", "a"]],
    ["unknown route", ["--install-source", "assayMcp", "sideloaded", "a"]],
    ["newline in reference", ["--install-source", "assayMcp", "crates-io", "a\nREADY"]],
    ["duplicate subject", [
      "--install-source", "assayMcp", "crates-io", "a",
      "--install-source", "assayMcp", "crates-io", "b",
    ]],
  ]) {
    assert.throws(() => parseArgs(argv), /--install-source/, `${label} must be refused`);
  }
});

test("#2684: the projection retains install source, version, os and arch", () => {
  // This is the regression that lost cell 1: the producer probed version and
  // installSource, and projectHostIdentity dropped both on the way to disk.
  const identity = {
    os: "darwin",
    arch: "arm64",
    codex: {
      path: "/proof/codex.snapshot",
      version: "codex 0.153.4",
      sha256: "a".repeat(64),
      installSource: { route: "host-bundled", reference: "Codex.app" },
    },
    codexCodeModeHost: {
      path: "/proof/codex-code-mode-host",
      sha256: "b".repeat(64),
      installSource: { route: "host-bundled", reference: "Codex.app" },
    },
    assayMcp: {
      path: "/proof/assay-mcp-server.snapshot",
      version: "assay-mcp-server 6.0.0",
      sha256: "c".repeat(64),
      installSource: { route: "crates-io", reference: "assay-mcp-server@6.0.0" },
    },
  };
  const projected = projectHostIdentity(identity);
  assert.equal(projected.os, "darwin");
  assert.equal(projected.arch, "arm64");
  assert.equal(projected.codex.version, "codex 0.153.4");
  assert.equal(projected.assayMcp.version, "assay-mcp-server 6.0.0");
  assert.deepEqual(projected.assayMcp.installSource, {
    route: "crates-io",
    reference: "assay-mcp-server@6.0.0",
  });
  // A subject without a probed version keeps the shorter shape rather than a null.
  assert.deepEqual(
    Object.keys(projected.codexCodeModeHost).sort(),
    ["installSource", "path", "sha256"],
  );
  // The projected identity must still satisfy the live binding contract.
  assert.equal(liveIdentityBound(projected), true);
  // ... and lose it if the install source is stripped, so the field is load bearing.
  const stripped = JSON.parse(JSON.stringify(projected));
  delete stripped.assayMcp.installSource;
  assert.equal(liveIdentityBound(stripped), false);
});

test("#2822 F1: reference and version are printable ASCII, not merely control-free", () => {
  const cp = (n) => String.fromCodePoint(n);
  // Positive control: an ordinary reference stays acceptable.
  assert.equal(installSourceBound({ route: "crates-io", reference: "assay-mcp-server@6.0.0" }), true);
  assert.equal(boundedPrintable("codex 0.153.4"), true);

  // The blocklist this replaced admitted every one of these. Named individually so a
  // regression says which class came back rather than only that something did.
  const hostile = [
    ["U+0085 NEL", 0x85],
    ["U+200B zero-width space", 0x200b],
    ["U+200E left-to-right mark", 0x200e],
    ["U+2028 line separator", 0x2028],
    ["U+2029 paragraph separator", 0x2029],
    ["U+202E right-to-left override", 0x202e],
    ["U+2066 left-to-right isolate", 0x2066],
    ["U+2069 pop directional isolate", 0x2069],
    ["U+FEFF byte order mark", 0xfeff],
    ["U+0000 NUL", 0x00],
    ["U+000A newline", 0x0a],
    ["U+007F DEL", 0x7f],
  ];
  for (const [label, point] of hostile) {
    const reference = `a${cp(point)}b`;
    assert.equal(
      installSourceBound({ route: "crates-io", reference }),
      false,
      `${label} must be refused in an install reference`,
    );
    assert.equal(boundedPrintable(reference), false, `${label} must be refused in a version`);
  }

  // The same rule reaches version through the binding contract.
  const withVersion = (version) => liveIdentityBound({
    os: "darwin",
    arch: "arm64",
    codex: {
      path: "/proof/codex.snapshot",
      version,
      sha256: "a".repeat(64),
      installSource: { route: "host-bundled", reference: "Codex.app" },
    },
    codexCodeModeHost: {
      path: "/proof/codex-code-mode-host",
      sha256: "b".repeat(64),
      installSource: { route: "host-bundled", reference: "Codex.app" },
    },
    assayMcp: {
      path: "/proof/assay-mcp-server.snapshot",
      version: "assay-mcp-server 6.0.0",
      sha256: "c".repeat(64),
      installSource: { route: "crates-io", reference: "assay-mcp-server@6.0.0" },
    },
  });
  assert.equal(withVersion("codex 0.153.4"), true);
  assert.equal(withVersion(`codex${cp(0x2028)}0.153.4`), false);
});

test("#2822 F1: the printable-ASCII contract is exactly U+0020-U+007E, swept", () => {
  const cp = (n) => String.fromCodePoint(n);
  // Completeness is proven rather than asserted: for every code point in the BMP the
  // validator's answer must agree with the stated contract, in both directions. A
  // blocklist cannot pass this; only an allowlist can.
  let checked = 0;
  for (let point = 0; point <= 0xffff; point += 1) {
    if (point >= 0xd800 && point <= 0xdfff) {
      continue; // lone surrogates are not scalar values
    }
    const inContract = point >= 0x20 && point <= 0x7e;
    const value = `a${cp(point)}b`;
    if (boundedPrintable(value) !== inContract) {
      assert.fail(
        `U+${point.toString(16).toUpperCase().padStart(4, "0")} disagrees with the contract: ` +
        `expected ${inContract ? "accepted" : "refused"}`,
      );
    }
    checked += 1;
  }
  assert.equal(checked, 0x10000 - 0x800, "the sweep must cover every BMP scalar value");

  // Astral planes are outside the contract too, including emoji and tag characters.
  for (const point of [0x1f600, 0xe0001, 0xe0041, 0x10ffff]) {
    assert.equal(boundedPrintable(`a${cp(point)}b`), false, `U+${point.toString(16)} must be refused`);
  }

  // The full agreed bidi ranges, named so a regression reports which range returned.
  for (const [label, lo, hi] of [
    ["bidi embeddings and overrides U+202A-U+202E", 0x202a, 0x202e],
    ["bidi isolates U+2066-U+2069", 0x2066, 0x2069],
    ["zero-width and marks U+200B-U+200F", 0x200b, 0x200f],
  ]) {
    for (let point = lo; point <= hi; point += 1) {
      const reference = `assay-mcp-server@6.0.0${cp(point)}`;
      assert.equal(
        installSourceBound({ route: "crates-io", reference }),
        false,
        `${label}: U+${point.toString(16).toUpperCase()} must be refused in a reference`,
      );
    }
  }

  // Positive controls: the vocabulary references actually used must all remain valid.
  for (const reference of [
    "assay-mcp-server@6.0.0",
    "v6.0.0/assay-v6.0.0-aarch64-apple-darwin.tar.gz",
    "Codex.app",
    "assay-mcp-server-v6.0.0-x86_64-unknown-linux-gnu.tar.gz",
    "a".repeat(200),
  ]) {
    assert.equal(installSourceBound({ route: "github-release", reference }), true, reference.slice(0, 40));
  }
  assert.equal(installSourceBound({ route: "github-release", reference: "a".repeat(201) }), false);
});
