#!/usr/bin/env node
/**
 * Bounded Codex app-server host-proof driver. Spawns a stdio child, records
 * sanitized events, and classifies them with the validator's shared function.
 * Does not copy profiles or start a live model turn unless explicitly allowed.
 *
 * Byte cap applies before parse/retention. On cap or parse failure, stdin is
 * ended and the child is SIGTERM'd, then SIGKILL after 1s if still alive.
 * That is process cleanup, not a sandbox.
 */
import { execFileSync, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  CODEX_APP_SERVER_ARGS,
  DECIDE_INPUT,
  DECIDE_TOOL,
  EXPECTED_TOOLS,
  HARD_MAX_BYTES,
  HARD_MAX_EVENTS,
  HARD_MAX_FRAMES,
  HARD_MAX_RETAINED_BYTES,
  HARD_MAX_SNAPSHOT_BYTES,
  HARD_MAX_TIMEOUT_MS,
  HOST_ENV_NAMES,
  HOST_SUBJECTS,
  PACKAGE_SUBJECTS,
  SCHEMA,
  SKILL_NAME,
  boundedPositiveInt,
  classifyRecord,
  closedDriverOutcomeStatus,
  consumeBoundedBinary,
  consumeJourneyTopology,
  credentialArgvReason,
  decidePrompt,
  driverOutcomeFrom,
  elicitationAcceptable,
  forbiddenProofRoot,
  initializeFromEvents,
  hostSubjectsRequired,
  runtimeProofRoots,
  isMainModule,
  persistableArgv,
  CWD_DIFFERS_FROM_PROJECT_ROOT,
  CWD_MATCHES_PROJECT_ROOT,
  projectRetainedEvent,
  installSourceBound,
  assayPackageVersion,
  packageFiles,
  verifyAssayPackage,
  readBoundedFile,
  parseJsonBytes,
  projectHostIdentity,
  proofAllowlist,
  requirePrivateDirectory,
  requirePrivateProofRoot,
  requiredCellsForJourney,
  resolvePendingResponse,
  sha256File,
  sha256Utf8,
  stableStringify,
  validateProofRoot,
  walkDepth,
} from "./codex_host_proof_validator.mjs";

const INSTALL_SUBJECTS = Object.freeze(["codex", "codexCodeModeHost", "assayMcp"]);
const PACKAGE_VERIFICATION = Symbol("producer-package-verification");

const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_BYTES = 1_048_576;

export function parseArgs(argv) {
  const out = {
    captureMode: "synthetic-fixture",
    timeoutMs: DEFAULT_TIMEOUT_MS,
    maxBytes: DEFAULT_MAX_BYTES,
    journey: "tool",
    allowLiveTurn: false,
    childArgv: null,
    proofRoot: null,
    projectRoot: null,
    installSources: {},
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = () => {
      i += 1;
      return argv[i];
    };
    switch (arg) {
      case "--capture-mode":
        out.captureMode = next();
        break;
      case "--proof-root":
        out.proofRoot = next();
        break;
      case "--project-root":
        out.projectRoot = next();
        break;
      case "--journey":
        out.journey = next();
        break;
      case "--timeout-ms":
        out.timeoutMs = Number(next());
        break;
      case "--max-bytes":
        out.maxBytes = Number(next());
        break;
      case "--allow-live-turn":
        out.allowLiveTurn = true;
        break;
      case "--assay-package":
      case "--assay-index-row":
      case "--assay-cargo-metadata": {
        const key = { "--assay-package": "package", "--assay-index-row": "index", "--assay-cargo-metadata": "installation" }[arg];
        out.packageInputs ??= {};
        if (out.packageInputs[key]) throw new Error(`duplicate ${arg}`);
        const value = next();
        if (!value || !path.isAbsolute(value)) throw new Error(`${arg} requires an absolute file path`);
        out.packageInputs[key] = value;
        break;
      }
      // Install source is declared, never inferred: how a binary got onto this
      // machine is not observable from the binary the host launches. Each token is
      // validated structurally here so nothing free-form reaches the record.
      case "--install-source": {
        const subject = next();
        const route = next();
        const reference = next();
        if (!INSTALL_SUBJECTS.includes(subject)) {
          throw new Error(`--install-source subject must be one of ${INSTALL_SUBJECTS.join(", ")}`);
        }
        if (Object.prototype.hasOwnProperty.call(out.installSources, subject)) {
          throw new Error(`--install-source declared twice for ${subject}`);
        }
        const candidate = { route, reference };
        if (!installSourceBound(candidate)) {
          throw new Error(`--install-source for ${subject} is not a valid route and reference`);
        }
        out.installSources[subject] = candidate;
        break;
      }
      default:
        throw new Error(`unknown argument ${arg}`);
    }
  }
  return out;
}

function writeJson(file, value) {
  const dir = path.dirname(file);
  const tmp = path.join(dir, `.${randomBytes(16).toString("hex")}.tmp`);
  const flags =
    fs.constants.O_CREAT |
    fs.constants.O_EXCL |
    fs.constants.O_WRONLY |
    (fs.constants.O_NOFOLLOW || 0);
  const fd = fs.openSync(tmp, flags, 0o600);
  try {
    fs.writeFileSync(fd, stableStringify(value));
  } finally {
    fs.closeSync(fd);
  }
  fs.renameSync(tmp, file);
}

function requireFreshProofRoot(proofRoot) {
  if (fs.existsSync(proofRoot)) {
    const st = fs.lstatSync(proofRoot);
    if (st.isSymbolicLink() || !st.isDirectory()) {
      throw new Error("proof root must be a fresh directory");
    }
    const dir = fs.opendirSync(proofRoot);
    try {
      let entry;
      while ((entry = dir.readSync()) !== null) {
        if (entry.name === "." || entry.name === "..") {
          continue;
        }
        throw new Error("proof root must be a fresh empty directory");
      }
    } finally {
      dir.closeSync();
    }
  } else {
    fs.mkdirSync(proofRoot, { recursive: true, mode: 0o700 });
  }
  return requirePrivateProofRoot(proofRoot);
}

function requireOrCreatePrivateCodexHome(projectRoot) {
  const codexHome = path.join(projectRoot, ".codex-home");
  if (!fs.existsSync(codexHome)) {
    fs.mkdirSync(codexHome, { mode: 0o700 });
  }
  return requirePrivateDirectory(codexHome, "CODEX_HOME");
}

function encode(message) {
  return `${JSON.stringify(message)}\n`;
}

class BoundBuffer {
  constructor(maxBytes) {
    this.maxBytes = maxBytes;
    this.chunks = [];
    this.size = 0;
    this.truncated = false;
  }

  push(chunk) {
    if (this.truncated) {
      return;
    }
    if (this.size + chunk.length > this.maxBytes) {
      this.truncated = true;
      const remain = this.maxBytes - this.size;
      if (remain > 0) {
        this.chunks.push(chunk.subarray(0, remain));
        this.size += remain;
      }
      return;
    }
    this.chunks.push(chunk);
    this.size += chunk.length;
  }

  text() {
    return Buffer.concat(this.chunks, this.size).toString("utf8");
  }
}

function parseLine(line, maxBytes) {
  if (Buffer.byteLength(line, "utf8") > maxBytes) {
    const error = new Error("rpc line exceeds byte bound");
    error.truncated = true;
    throw error;
  }
  const value = JSON.parse(line);
  walkDepth(value);
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("rpc line is not an object");
  }
  return value;
}

function resolveRegularBinary(binPath, label) {
  if (typeof binPath !== "string" || binPath.length === 0) {
    throw new Error(`${label} binary must be an absolute path`);
  }
  const resolved = path.resolve(binPath);
  if (!path.isAbsolute(resolved)) {
    throw new Error(`${label} binary must be an absolute path`);
  }
  const st = fs.lstatSync(resolved);
  if (st.isSymbolicLink() || !st.isFile()) {
    throw new Error(`${label} binary must be a non-symlink regular file`);
  }
  if (process.platform !== "win32" && (st.mode & 0o111) === 0) {
    throw new Error(`${label} binary must be executable`);
  }
  fs.accessSync(resolved, fs.constants.X_OK);
  return fs.realpathSync(resolved);
}

function findOnPath(name) {
  for (const dir of (process.env.PATH || "").split(path.delimiter)) {
    if (!dir) {
      continue;
    }
    const candidate = path.join(dir, name);
    try {
      const st = fs.lstatSync(candidate);
      if (st.isSymbolicLink()) {
        const real = fs.realpathSync(candidate);
        if (fs.lstatSync(real).isFile()) {
          return real;
        }
        continue;
      }
      if (st.isFile()) {
        return candidate;
      }
    } catch {
      /* next */
    }
  }
  return null;
}

const VERSION_PROBE = Object.freeze({
  encoding: "utf8",
  timeout: 2000,
  maxBuffer: 4096,
});

function firstVersionLine(result, name) {
  const text = `${result.stdout || ""}${result.stderr || ""}`.trim().split(/\r?\n/)[0] || "";
  if (!text) {
    throw new Error(`could not observe version from ${name}`);
  }
  return text.slice(0, 200);
}

const BOUND_EXEC = Symbol("boundExec");

function snapshotNamedBinary(srcPath, destDir, destName, options = {}) {
  fs.mkdirSync(destDir, { recursive: true, mode: 0o700 });
  const dest = path.join(destDir, destName);
  const src = resolveRegularBinary(srcPath, destName);
  const maxBytes = options.testOnlySnapshotMaxBytes ?? HARD_MAX_SNAPSHOT_BYTES;
  let outFd = null;
  try {
    consumeBoundedBinary(
      src,
      maxBytes,
      (chunk) => {
        fs.writeSync(outFd, chunk);
      },
      {
        afterStat() {
          const outFlags =
            fs.constants.O_CREAT |
            fs.constants.O_EXCL |
            fs.constants.O_WRONLY |
            (fs.constants.O_NOFOLLOW || 0);
          outFd = fs.openSync(dest, outFlags, 0o700);
        },
        testOnlyAfterRead(copied, file) {
          if (typeof options.testOnlyAfterSnapshotRead === "function") {
            options.testOnlyAfterSnapshotRead(copied, file, destName);
          }
        },
      },
    );
  } catch (error) {
    if (outFd != null) {
      try {
        fs.closeSync(outFd);
      } catch {
        /* already closed */
      }
      outFd = null;
    }
    try {
      fs.unlinkSync(dest);
    } catch {
      /* dest may be absent */
    }
    throw error;
  } finally {
    if (outFd != null) {
      fs.closeSync(outFd);
    }
  }
  fs.chmodSync(dest, 0o700);
  return dest;
}

function probeVersion(binary, name) {
  try {
    // This executes the fixed-name, O_EXCL-created proof snapshot with fixed
    // argv and no shell. Choosing the PATH subject is the host measurement.
    const stdout = execFileSync(binary, ["--version"], VERSION_PROBE);
    return firstVersionLine({ stdout }, name);
  } catch (error) {
    throw new Error(`${name} version probe failed`, { cause: error });
  }
}

function probeCodexVersion(binary) {
  return probeVersion(binary, "codex");
}

function probeAssayMcpVersion(binary) {
  return probeVersion(binary, "assay-mcp-server");
}

function retainAssayPackage(options, mcpPath, mcpSnapshot) {
  const input = options.packageInputs;
  if (!input?.package || !input.index || !input.installation) throw new Error("Assay package inputs are required before execution");
  if (path.basename(input.installation) !== ".crates2.json" ||
      fs.realpathSync(path.join(path.dirname(input.installation), "bin", "assay-mcp-server")) !== fs.realpathSync(mcpPath)) {
    throw new Error("Assay package installation metadata must belong to the selected binary prefix");
  }
  const source = options.installSources.assayMcp;
  const version = assayPackageVersion(source);
  const key = `assay-mcp-server ${version} (registry+https://github.com/rust-lang/crates.io-index)`;
  const cargo = parseJsonBytes(readBoundedFile(input.installation, HARD_MAX_BYTES), HARD_MAX_BYTES);
  const entry = cargo?.installs?.[key];
  if (!entry) throw new Error("Assay package is absent from Cargo installation metadata");
  const installation = { package: key, version_req: entry.version_req, bins: entry.bins,
    profile: entry.profile, target: entry.target, rustc: entry.rustc };
  const chunks = [readBoundedFile(input.package, HARD_MAX_BYTES),
    readBoundedFile(input.index, 256 * 1024), Buffer.from(stableStringify(installation))];
  const created = [];
  try {
    for (let i = 0; i < PACKAGE_SUBJECTS.length; i += 1) {
      const file = path.join(options.proofRoot, PACKAGE_SUBJECTS[i]);
      fs.writeFileSync(file, chunks[i], { flag: "wx", mode: 0o600 });
      created.push(file);
    }
    return { ...verifyAssayPackage(packageFiles(options.proofRoot), source), binarySha256: sha256File(mcpSnapshot) };
  } catch (error) {
    for (const file of created) fs.unlinkSync(file);
    throw error;
  }
}

export function resolveHostIdentity(options = {}) {
  const declared = options.installSources ?? {};
  const missing = INSTALL_SUBJECTS.filter((s) => !declared[s]);
  if (missing.length > 0) {
    // Fail closed. A record without a declared install source cannot satisfy the
    // #2684 "record install source" cell, and an absent field must not read as a pass.
    throw new Error(`--install-source is required for: ${missing.join(", ")}`);
  }
  const codexPath = findOnPath("codex");
  const mcpPath = findOnPath("assay-mcp-server");
  if (!codexPath) {
    throw new Error("codex binary was not resolved");
  }
  if (!mcpPath) {
    throw new Error("assay MCP binary was not resolved");
  }
  if (typeof options.proofRoot !== "string" || options.proofRoot.length === 0) {
    throw new Error("proofRoot is required for proof-owned host subjects");
  }
  const codexSource = resolveRegularBinary(codexPath, "codex");
  const codeModeHostName = HOST_SUBJECTS[1];
  const codeModeHostSource = path.join(path.dirname(codexSource), codeModeHostName);
  const snapRoot = requirePrivateProofRoot(options.proofRoot);
  const codexSnap = path.join(snapRoot, "codex.snapshot");
  const codeModeHostSnap = path.join(snapRoot, codeModeHostName);
  const mcpSnap = path.join(snapRoot, "assay-mcp-server.snapshot");
  if (fs.existsSync(codexSnap) || fs.existsSync(codeModeHostSnap) || fs.existsSync(mcpSnap)) {
    throw new Error("proof-owned host subject already exists");
  }
  try {
    const snapOpts = {
      testOnlySnapshotMaxBytes: options.testOnlySnapshotMaxBytes,
      testOnlyAfterSnapshotRead: options.testOnlyAfterSnapshotRead,
    };
    snapshotNamedBinary(codexSource, snapRoot, "codex.snapshot", snapOpts);
    snapshotNamedBinary(codeModeHostSource, snapRoot, codeModeHostName, snapOpts);
    snapshotNamedBinary(mcpPath, snapRoot, "assay-mcp-server.snapshot", snapOpts);
    // Before either --version probe: a host executable can itself start Assay.
    const packageVerification = options.captureMode === "host-observation"
      ? retainAssayPackage(options, mcpPath, mcpSnap) : null;
    const identity = {
      os: os.platform(),
      arch: os.arch(),
      codex: {
        path: fs.realpathSync(codexSnap),
        version: probeCodexVersion(codexSnap),
        sha256: sha256File(codexSnap),
        installSource: declared.codex,
      },
      codexCodeModeHost: {
        path: fs.realpathSync(codeModeHostSnap),
        sha256: sha256File(codeModeHostSnap),
        installSource: declared.codexCodeModeHost,
      },
      assayMcp: {
        path: fs.realpathSync(mcpSnap),
        version: probeAssayMcpVersion(mcpSnap),
        sha256: sha256File(mcpSnap),
        installSource: declared.assayMcp,
      },
    };
    identity[BOUND_EXEC] = {
      codexPath: codexSnap,
      mcpPath: mcpSnap,
    };
    identity[PACKAGE_VERIFICATION] = packageVerification;
    if (packageVerification && identity.assayMcp.version !== `assay-mcp-server ${packageVerification.version}`) {
      throw new Error("Assay package version and probed binary version disagree");
    }
    return identity;
  } catch (error) {
    for (const subject of [codexSnap, codeModeHostSnap, mcpSnap]) {
      try {
        fs.rmSync(subject, { force: true });
      } catch {
        /* acquisition failure must not leave a partial subject */
      }
    }
    throw error;
  }
}

function resolvedMcpCommand(options) {
  if (options.hostIdentity?.assayMcp?.path) {
    return options.hostIdentity.assayMcp.path;
  }
  if (
    options.testOnlyChild &&
    typeof options.assayMcpBin === "string" &&
    options.assayMcpBin.length > 0
  ) {
    return path.resolve(options.assayMcpBin);
  }
  const fromPath = findOnPath("assay-mcp-server");
  if (fromPath) {
    return fromPath;
  }
  throw new Error("assay MCP binary was not resolved");
}

// Retention-only identity projection, applied at the ONE boundary where the proof is written.
//
// It cannot live at `retainEvent`: the driver reads its own live wire identities back out of that
// array (`started[0].result.thread.id`, `response.result.thread.id`), so relabelling there would
// put tokens on the wire. Here the in-memory array stays raw for the driver and only the persisted
// copy is projected -- and because the manifest hash, the classification and all three files are
// derived from the same projected copy, they cannot disagree.
//
// Roles are separate maps with separate prefixes, so two namespaces are never merged by accident.
// Fields that ARE one identity deliberately share a role: `params.threadId` with
// `result.thread.id`, and `params.turnId` with `params.turn.id`, because the cells compare exactly
// those pairs. Every site is an explicit named path from the identifier disposition table -- never
// a recursive walk, which would rename fields nobody enumerated.
//
// Every comparison downstream is an equality test, which is why a first-seen ordinal is
// information-preserving for them: classifying the projected copy yields the same cells as
// classifying the raw one.
const RETAINED_ID_PREFIX = Object.freeze({ rpc: "#", thread: "@", turn: "~", item: "!" });

function retainedIdentityProjection(events, projectRoot) {
  const seen = new Map(Object.keys(RETAINED_ID_PREFIX).map((role) => [role, new Map()]));
  const isObj = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
  const own = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const token = (role, value) => {
    if (typeof value !== "string" || value.length === 0) {
      return value;
    }
    const roleSeen = seen.get(role);
    if (!roleSeen.has(value)) {
      roleSeen.set(value, `${RETAINED_ID_PREFIX[role]}${roleSeen.size}`);
    }
    return roleSeen.get(value);
  };
  return events.map((original) => {
    const event = structuredClone(original);
    if (own(event, "id")) {
      event.id = token("rpc", event.id);
    }
    const params = event.params;
    if (isObj(params)) {
      if (own(params, "threadId")) {
        params.threadId = token("thread", params.threadId);
      }
      if (own(params, "turnId")) {
        params.turnId = token("turn", params.turnId);
      }
      if (isObj(params.item) && own(params.item, "id")) {
        params.item.id = token("item", params.item.id);
      }
      if (isObj(params.turn)) {
        if (own(params.turn, "id")) {
          params.turn.id = token("turn", params.turn.id);
        }
        if (Array.isArray(params.turn.items)) {
          for (const entry of params.turn.items) {
            if (isObj(entry) && own(entry, "id")) {
              entry.id = token("item", entry.id);
            }
          }
        }
      }
    }
    const result = event.result;
    if (isObj(result)) {
      if (isObj(result.thread) && own(result.thread, "id")) {
        result.thread.id = token("thread", result.thread.id);
      }
      // `result.turn.id` is the OTHER member of the turn namespace. Missing it is what a partial
      // namespace looks like: `params.turn.id` becomes a token while this stays raw, and the cell
      // that compares exactly those two stops matching. Every member of a role must be listed.
      if (isObj(result.turn) && own(result.turn, "id")) {
        result.turn.id = token("turn", result.turn.id);
      }
      // Group D: record the project-root COMPARISON outcome, never the host path.
      if (typeof result.cwd === "string") {
        result.cwd =
          result.cwd === projectRoot
            ? CWD_MATCHES_PROJECT_ROOT
            : CWD_DIFFERS_FROM_PROJECT_ROOT;
      }
    }
    return event;
  });
}

function writeProofFiles(options, pack) {
  pack = { ...pack, captureMode: options.captureMode, events: retainedIdentityProjection(pack.events, options.projectRoot) };
  const initialize = initializeFromEvents(pack.events, options.journey);
  const hostIdentity = projectHostIdentity(options.hostIdentity ?? null);
  const invocationArgv = persistableArgv(
    options.hostIdentity?.codex?.path
      ? [options.hostIdentity.codex.path, ...CODEX_APP_SERVER_ARGS]
      : Array.isArray(options.childArgv)
        ? options.childArgv
        : ["<test-only-child>"],
  );
  const record = {
    schema: SCHEMA,
    captureMode: options.captureMode,
    journey: options.journey,
    childExitCode: pack.childExit,
    driverOutcome: null,
    truncated: pack.truncated,
    streamUnavailable: pack.streamUnavailable,
    initialize,
    hostIdentity,
    packageVerification: options.hostIdentity?.[PACKAGE_VERIFICATION] ?? null,
    proofRoot: options.proofRoot,
    invocation: { argv: invocationArgv, envNames: [...HOST_ENV_NAMES] },
    expected: {
      projectRoot: options.projectRoot,
      skillName: SKILL_NAME,
      tools: [...EXPECTED_TOOLS],
      toolName: DECIDE_TOOL,
      toolArguments: DECIDE_INPUT,
    },
    events: pack.events,
  };
  consumeJourneyTopology(pack.events, options.journey);
  const preliminary = classifyRecord(record);
  let driverOutcome = driverOutcomeFrom(pack, preliminary.cells, options.journey);
  const outcomeKind = closedDriverOutcomeStatus({
    childExitCode: pack.childExit,
    driverOutcome,
    truncated: pack.truncated,
    streamUnavailable: pack.streamUnavailable,
  });
  if (outcomeKind === "invalid") {
    driverOutcome = {
      exitCode: driverOutcome.exitCode === 0 ? 1 : driverOutcome.exitCode,
      status: "fail",
    };
  }
  record.driverOutcome = driverOutcome;
  let classified = classifyRecord(record);
  const eventsText = stableStringify(pack.events);
  const manifest = {
    schema: SCHEMA,
    captureMode: options.captureMode,
    journey: options.journey,
    childExitCode: pack.childExit,
    driverOutcome,
    truncated: pack.truncated,
    streamUnavailable: pack.streamUnavailable,
    bounds: {
      timeoutMs: options.timeoutMs,
      maxBytes: options.maxBytes,
      stdoutBytes: pack.stdoutBytes,
      stderrBytes: pack.stderrBytes,
    },
    invocation: record.invocation,
    initialize,
    hostIdentity,
    packageVerification: record.packageVerification,
    expected: record.expected,
    hashes: { events: sha256Utf8(eventsText) },
    allowlist: [
      ...proofAllowlist(hostSubjectsRequired(options.captureMode, options.hostIdentity), record.packageVerification != null),
    ].sort(),
  };
  writeJson(path.join(options.proofRoot, "manifest.json"), manifest);
  writeJson(path.join(options.proofRoot, "events.json"), pack.events);
  writeJson(path.join(options.proofRoot, "classification.json"), classified);
  let checked = validateProofRoot(options.proofRoot);
  if (!checked.ok) {
    driverOutcome = {
      exitCode: driverOutcome.exitCode === 0 ? 1 : driverOutcome.exitCode,
      status: "fail",
    };
    manifest.driverOutcome = driverOutcome;
    record.driverOutcome = driverOutcome;
    classified = classifyRecord(record);
    writeJson(path.join(options.proofRoot, "manifest.json"), manifest);
    writeJson(path.join(options.proofRoot, "classification.json"), classified);
    checked = validateProofRoot(options.proofRoot);
  }
  return {
    manifest,
    events: pack.events,
    classified: checked.classified ?? classified,
    childExitCode: pack.childExit,
    driverOutcome,
    recordConsistency: checked.recordConsistency ?? null,
    externalAttestation: checked.externalAttestation ?? "not_provided",
  };
}

function writePreSpawnFailure(options, message) {
  return writeProofFiles(options, {
    events: [
      projectRetainedEvent({
        direction: "driver",
        method: "pre-spawn-error",
        params: { message },
      }),
    ],
    childExit: 1,
    truncated: false,
    streamUnavailable: true,
    stdoutBytes: 0,
    stderrBytes: 0,
  });
}

export async function runProof(options) {
  requiredCellsForJourney(options.journey);
  const forbidden = forbiddenProofRoot(
    options.proofRoot,
    options.captureMode,
    runtimeProofRoots(options.projectRoot),
  );
  if (forbidden) {
    if (options.testOnlyChild) {
      try {
        options.testOnlyChild.kill("SIGTERM");
      } catch {
        /* already exited */
      }
    }
    throw new Error(forbidden);
  }
  if (options.captureMode === "host-observation" && options.journey === "tool" && !options.allowLiveTurn) {
    throw new Error("host-observation tool journey requires --allow-live-turn; this slice does not authorize a model call");
  }
  if (options.captureMode !== "synthetic-fixture" && options.captureMode !== "host-observation") {
    throw new Error("captureMode must be synthetic-fixture or host-observation");
  }
  boundedPositiveInt("timeoutMs", options.timeoutMs, HARD_MAX_TIMEOUT_MS);
  boundedPositiveInt("maxBytes", options.maxBytes, HARD_MAX_BYTES);

  const runDeadline = Date.now() + options.timeoutMs;
  options.proofRoot = requireFreshProofRoot(options.proofRoot);
  const credential = Array.isArray(options.childArgv)
    ? credentialArgvReason(options.childArgv)
    : null;
  if (credential) {
    return writePreSpawnFailure(options, credential);
  }

  if (!options.testOnlyChild && !options.hostIdentity) {
    options.hostIdentity = resolveHostIdentity({
      proofRoot: options.proofRoot,
      installSources: options.installSources,
      captureMode: options.captureMode,
      packageInputs: options.packageInputs,
    });
  }
  if (options.captureMode === "host-observation") {
    // A supplied identity must not bypass the production acquisition gate.
    const expected = options.hostIdentity?.[PACKAGE_VERIFICATION];
    if (!expected || stableStringify({
      ...verifyAssayPackage(packageFiles(options.proofRoot), options.hostIdentity.assayMcp.installSource),
      binarySha256: sha256File(options.hostIdentity.assayMcp.path),
    }) !== stableStringify(expected)) {
      if (options.testOnlyChild) options.testOnlyChild.kill("SIGTERM");
      throw new Error("Assay package verification is missing or changed before host execution");
    }
  }
  let codexHome;
  try {
    codexHome = requireOrCreatePrivateCodexHome(options.projectRoot);
  } catch (error) {
    return writePreSpawnFailure(options, String(error));
  }
  const events = [];
  const stdout = new BoundBuffer(options.maxBytes);
  const stderr = new BoundBuffer(options.maxBytes);
  let nextId = 1;
  let streamUnavailable = false;
  let stopped = false;
  let childAlive = true;
  let frames = 0;
  let retainedBytes = 0;
  let childExit = null;
  let acceptedElicitations = 0;
  const pending = new Map();
  // A STRING rpc id can only have come from the host: the driver's own requests take their ids from
  // `nextId`, which is always an integer. So every retained string id is host text, exactly like a
  // method name or a key name. The converse does NOT hold -- see the note on integers below.
  //
  // The wire is untouched -- the child still sees, and we still answer with, the real id, so string
  // ids remain fully supported on the protocol. Only what is RETAINED is relabelled, through one
  // per-run map at `retainEvent`, the single funnel every retained event already passes through.
  // (The one projection outside it, `writePreSpawnFailure`, carries no id and never contacts a host.)
  //
  // First-seen ordinal, deliberately NOT a hash. It preserves exactly the structure correlation
  // needs -- the same id always yields the same token, distinct ids always yield distinct tokens --
  // so a server request and its matching client response still pair, and a reused id is still
  // visibly reused. It carries no preimage and is not offered as reversible redaction: what the
  // proof retains is correlation structure, not identity.
  //
  // Integers pass through unchanged -- but NOT because they are ours. A host may choose a numeric
  // id too; what is true is only that driver-ORIGINATED requests always take integers from
  // `nextId`, which is not the converse. They are preserved because the protocol requires numeric
  // ids to stay valid on the wire and an integer carries no text, not because their provenance is
  // known. Preserving them is also what keeps the client-generated integer response lookup working.
  const retainEvent = (event) => {
    const encoded = Buffer.byteLength(JSON.stringify(event), "utf8");
    if (
      events.length >= HARD_MAX_EVENTS ||
      retainedBytes + encoded > HARD_MAX_RETAINED_BYTES
    ) {
      stdout.truncated = true;
      stopChild();
      return false;
    }
    events.push(event);
    retainedBytes += encoded;
    return true;
  };

  const bound = options.hostIdentity?.[BOUND_EXEC];
  const spawnOpts = {
    stdio: ["pipe", "pipe", "pipe"],
    env: {
      PATH: process.env.PATH,
      HOME: options.projectRoot,
      CODEX_HOME: codexHome,
    },
  };
  const child = options.testOnlyChild
    ? options.testOnlyChild
    : spawn(bound.codexPath, CODEX_APP_SERVER_ARGS, spawnOpts);
  const childClosed = new Promise((resolve) => {
    child.on("close", (code, signal) => {
      childAlive = false;
      resolve(code ?? (signal ? 1 : 0));
    });
  });
  const stopChild = () => {
    if (stopped) {
      return;
    }
    stopped = true;
    try {
      child.stdin.end();
    } catch {
      /* already closed */
    }
    try {
      child.kill("SIGTERM");
    } catch {
      /* already exited */
    }
  };
  child.stderr.on("data", (chunk) => {
    stderr.push(chunk);
    if (stderr.truncated) {
      stopChild();
    }
  });
  child.on("error", () => {
    streamUnavailable = true;
    stopChild();
  });

  const childStdinIsWritable = () =>
    Boolean(
      !stopped &&
        childAlive &&
        child.stdin &&
        !child.stdin.destroyed &&
        child.stdin.writable &&
        !child.stdin.writableEnded,
    );

  let stdinFailureNoted = false;
  const noteStdinFailure = (message) => {
    if (stdinFailureNoted) {
      return false;
    }
    stdinFailureNoted = true;
    streamUnavailable = true;
    retainEvent(
      projectRetainedEvent({
        direction: "driver",
        method: "error",
        params: { message },
      }),
    );
    stopChild();
    return false;
  };

  const onChildStdinError = () => {
    noteStdinFailure("child stdin error");
  };
  if (child.stdin) {
    child.stdin.on("error", onChildStdinError);
  }

  const dropClientWrite = (id) => {
    if (id != null) {
      pending.delete(id);
    }

    for (let i = events.length - 1; i >= 0; i -= 1) {
      const event = events[i];
      if (event.direction !== "client") {
        continue;
      }
      if (id != null ? event.id !== id : event.method !== "initialized") {
        continue;
      }
      const encoded = Buffer.byteLength(JSON.stringify(event), "utf8");
      events.splice(i, 1);
      retainedBytes = Math.max(0, retainedBytes - encoded);
      break;
    }
  };

  let inFlightWrites = 0;
  const writeChildStdin = (payload, onFailed) => {
    if (!childStdinIsWritable()) {
      noteStdinFailure("child stdin is not writable");
      onFailed?.();
      return false;
    }
    let encoded;
    try {
      encoded = encode(payload);
    } catch {
      noteStdinFailure("child stdin write failed");
      onFailed?.();
      return false;
    }
    inFlightWrites += 1;
    try {
      child.stdin.write(encoded, (err) => {
        inFlightWrites = Math.max(0, inFlightWrites - 1);
        if (err) {
          noteStdinFailure("child stdin error");
          onFailed?.();
        }
      });
    } catch {
      inFlightWrites = Math.max(0, inFlightWrites - 1);
      noteStdinFailure("child stdin write failed");
      onFailed?.();
      return false;
    }
    return true;
  };

  let buffer = "";
  const onLine = (line) => {
    frames += 1;
    if (frames > HARD_MAX_FRAMES) {
      stdout.truncated = true;
      stopChild();
      return;
    }
    const message = parseLine(line, options.maxBytes);
    const resolved = resolvePendingResponse(pending, message);
    if (resolved.kind === "resolve") {
      const event = {
        direction: "server",
        method: resolved.method,
        id: resolved.id,
      };
      if (resolved.result !== undefined) {
        event.result = resolved.result;
      }
      if (resolved.error !== undefined) {
        event.error = resolved.error;
      }
      retainEvent(projectRetainedEvent(event));
      return;
    }
    if (resolved.kind === "reject") {
      streamUnavailable = true;
      retainEvent(
        projectRetainedEvent({
          direction: "driver",
          method: "error",
          params: { message: resolved.reason },
        }),
      );
      stopChild();
      return;
    }
    if (typeof message.method === "string") {
      retainEvent(
        projectRetainedEvent({
          direction: "server",
          method: message.method,
          id: message.id ?? null,
          params: message.params ?? null,
        }),
      );
      if (message.method === "mcpServer/elicitation/request" && message.id != null && !stopped) {
        const started = events.filter(
          (event) => event.method === "thread/start" && event.direction === "server",
        );
        const threadId = started[0]?.result?.thread?.id;
        const turnReply = events.find(
          (event) => event.method === "turn/start" && event.direction === "server",
        );
        const turnId = turnReply?.result?.turn?.id;
        const accepted =
          elicitationAcceptable(message.params, threadId, turnId) && acceptedElicitations === 0;
        const reply = {
          id: message.id,
          result: {
            action: accepted ? "accept" : "decline",
            content: {},
          },
        };
        if (writeChildStdin(reply, () => dropClientWrite(message.id))) {
          if (accepted) {
            acceptedElicitations += 1;
          }
          retainEvent(
            projectRetainedEvent({
              direction: "client",
              method: message.method,
              id: message.id,
              result: reply.result,
            }),
          );
        }
      }
    }
  };

  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    if (stopped) {
      return;
    }
    stdout.push(Buffer.from(chunk, "utf8"));
    if (stdout.truncated) {
      buffer = "";
      stopChild();
      return;
    }
    buffer += chunk;
    try {
      let idx;
      while ((idx = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, idx).trim();
        buffer = buffer.slice(idx + 1);
        if (line) {
          onLine(line);
        }
      }
      if (Buffer.byteLength(buffer, "utf8") > options.maxBytes) {
        stdout.truncated = true;
        buffer = "";
        stopChild();
      }
    } catch (error) {
      buffer = "";
      if (error.truncated) {
        stdout.truncated = true;
      } else {
        streamUnavailable = true;
      }
      retainEvent(
        projectRetainedEvent({
          direction: "driver",
          method: "error",
          params: { message: "stdio parse failed" },
        }),
      );
      stopChild();
    }
  });

  const send = (method, params) => {
    const id = nextId;
    nextId += 1;
    pending.set(id, method);
    retainEvent(projectRetainedEvent({ direction: "client", method, id, params }));
    if (!writeChildStdin({ id, method, params }, () => dropClientWrite(id))) {
      dropClientWrite(id);
    }
    return id;
  };

  const blocked = () =>
    stdout.truncated || stderr.truncated || streamUnavailable || stopped || !childAlive;

  const waitFor = async (id) => {
    while (Date.now() < runDeadline) {
      if (events.some((event) => event.direction === "server" && event.id === id)) {
        return;
      }
      if (blocked()) {
        return;
      }
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    throw new Error(`timeout waiting for ${id}`);
  };

  const waitForTerminalTurn = async (threadId, turnId) => {
    while (Date.now() < runDeadline) {
      if (blocked()) {
        return;
      }
      const terminal = events.some(
        (event) =>
          event.method === "turn/completed" &&
          event.direction === "server" &&
          event.params?.threadId === threadId &&
          event.params?.turn?.id === turnId,
      );
      if (terminal) {
        return;
      }
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    throw new Error("timeout waiting for terminal turn");
  };

  try {
    send("initialize", {
      clientInfo: { name: "assay-codex-host-proof", version: "1" },
      capabilities: {},
    });
    await waitFor(1);
    const initialized = { method: "initialized", params: {} };
    if (writeChildStdin(initialized, () => dropClientWrite())) {
      retainEvent(projectRetainedEvent({ direction: "client", ...initialized }));
    }
    send("skills/list", { forceReload: true, cwds: [options.projectRoot] });
    await waitFor(2);

    const startThread = (mcpCommand, args) => {
      return send("thread/start", {
        cwd: options.projectRoot,
        approvalPolicy: "on-request",
        config: {
          mcp_servers: {
            assay: { command: mcpCommand, args },
          },
        },
      });
    };

    const serverThreadId = (requestId) => {
      const response = events.find(
        (event) => event.direction === "server" && event.id === requestId,
      );
      const threadId = response?.result?.thread?.id;
      if (typeof threadId !== "string" || threadId.length === 0) {
        throw new Error(`no thread id in server response ${requestId}`);
      }
      return threadId;
    };

    if (options.journey !== "discovery") {
      const primary = startThread(resolvedMcpCommand(options), ["--policy-root", "."]);
      await waitFor(primary);
      const threadId = serverThreadId(primary);
      const statusId = send("mcpServerStatus/list", { threadId, detail: "toolsAndAuthOnly" });
      await waitFor(statusId);
      if (options.journey === "tool" || options.journey === "failures") {
        const missing = startThread(
          path.join(options.projectRoot, "missing-assay-mcp-server"),
          ["--policy-root", "."],
        );
        await waitFor(missing);
        const missingThread = serverThreadId(missing);
        await waitFor(
          send("mcpServerStatus/list", { threadId: missingThread, detail: "toolsAndAuthOnly" }),
        );
        const invalid = startThread(resolvedMcpCommand(options), [
          "--policy-root",
          path.join(options.projectRoot, "missing-policy-root"),
        ]);
        await waitFor(invalid);
        const invalidThread = serverThreadId(invalid);
        await waitFor(
          send("mcpServerStatus/list", { threadId: invalidThread, detail: "toolsAndAuthOnly" }),
        );
      }
      if (options.journey === "tool") {
        const turnReq = send("turn/start", {
          threadId,
          input: [{ type: "text", text: decidePrompt() }],
        });
        await waitFor(turnReq);
        const turnResponse = events.find(
          (event) => event.direction === "server" && event.id === turnReq,
        );
        if (turnResponse?.error) {
          throw new Error("turn/start failed");
        }
        const turnId = turnResponse?.result?.turn?.id;
        if (typeof turnId !== "string" || turnId.length === 0) {
          throw new Error("no turn id in server response result.turn.id");
        }
        await waitForTerminalTurn(threadId, turnId);
      }
    }
  } catch (error) {
    streamUnavailable = streamUnavailable || /timeout|unavailable/i.test(String(error));
    retainEvent(
      projectRetainedEvent({
        direction: "driver",
        method: "error",
        params: { message: String(error) },
      }),
    );
    stopChild();
  }

  if (!stopped) {
    try {
      child.stdin.end();
    } catch {
      /* already closed */
    }
  }
  const killer = setTimeout(() => {
    try {
      child.kill("SIGKILL");
    } catch {
      /* already exited */
    }
  }, 1000);
  childExit = await childClosed;
  clearTimeout(killer);
  while (inFlightWrites > 0 && Date.now() < runDeadline) {
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  if (inFlightWrites > 0) {
    noteStdinFailure("child stdin write incomplete");
  }

  return writeProofFiles(options, {
    events,
    childExit,
    truncated: stdout.truncated || stderr.truncated,
    streamUnavailable,
    stdoutBytes: stdout.size,
    stderrBytes: stderr.size,
  });
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (!options.proofRoot || !options.projectRoot) {
    throw new Error("--proof-root and --project-root are required");
  }
  return runProof(options);
}

if (isMainModule(process.argv[1], import.meta.url)) {
  main()
    .then((result) => {
      process.stdout.write(stableStringify({
        captureMode: result.manifest.captureMode,
        childExitCode: result.childExitCode,
        driverOutcome: result.driverOutcome,
        recordConsistency: result.recordConsistency,
        externalAttestation: result.externalAttestation,
      }));
      const exitCode = result.driverOutcome.exitCode;
      if (exitCode !== 0) {
        process.exitCode = exitCode;
      }
    })
    .catch((error) => {
      process.stderr.write(`${error.message}\n`);
      process.exitCode = 1;
    });
}
