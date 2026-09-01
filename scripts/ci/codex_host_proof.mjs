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
import { spawn, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
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
  projectRetainedEvent,
  projectHostIdentity,
  proofAllowlist,
  requiredCellsForJourney,
  resolvePendingResponse,
  sha256File,
  sha256Utf8,
  stableStringify,
  validateProofRoot,
  walkDepth,
} from "./codex_host_proof_validator.mjs";

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
  const st = fs.lstatSync(proofRoot);
  if (st.isSymbolicLink() || !st.isDirectory()) {
    throw new Error("proof root must be a real directory");
  }
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

function probeCodexVersion(binary) {
  return firstVersionLine(
    spawnSync(binary, ["--version"], VERSION_PROBE),
    "codex",
  );
}

function probeAssayMcpVersion(binary) {
  return firstVersionLine(
    spawnSync(binary, ["--version"], VERSION_PROBE),
    "assay-mcp-server",
  );
}

export function resolveHostIdentity(options = {}) {
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
  const snapRoot = path.resolve(options.proofRoot);
  const rootStat = fs.lstatSync(snapRoot);
  if (rootStat.isSymbolicLink() || !rootStat.isDirectory()) {
    throw new Error("proofRoot must be a real directory");
  }
  const codexSnap = path.join(snapRoot, "codex.snapshot");
  const mcpSnap = path.join(snapRoot, "assay-mcp-server.snapshot");
  if (fs.existsSync(codexSnap) || fs.existsSync(mcpSnap)) {
    throw new Error("proof-owned host subject already exists");
  }
  try {
    const snapOpts = {
      testOnlySnapshotMaxBytes: options.testOnlySnapshotMaxBytes,
      testOnlyAfterSnapshotRead: options.testOnlyAfterSnapshotRead,
    };
    snapshotNamedBinary(codexPath, snapRoot, "codex.snapshot", snapOpts);
    snapshotNamedBinary(mcpPath, snapRoot, "assay-mcp-server.snapshot", snapOpts);
    const identity = {
      os: os.platform(),
      arch: os.arch(),
      codex: {
        path: fs.realpathSync(codexSnap),
        version: probeCodexVersion(codexSnap),
        sha256: sha256File(codexSnap),
        installSource: "PATH",
      },
      assayMcp: {
        path: fs.realpathSync(mcpSnap),
        version: probeAssayMcpVersion(mcpSnap),
        sha256: sha256File(mcpSnap),
        installSource: "PATH",
      },
    };
    identity[BOUND_EXEC] = { codexPath: codexSnap, mcpPath: mcpSnap };
    return identity;
  } catch (error) {
    for (const subject of [codexSnap, mcpSnap]) {
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

function writeProofFiles(options, pack) {
  const initialize = initializeFromEvents(pack.events, options.journey);
  const hostIdentity = projectHostIdentity(options.hostIdentity ?? null);
  const invocationArgv = persistableArgv(
    options.hostIdentity?.codex?.path
      ? [options.hostIdentity.codex.path, "app-server"]
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
    expected: record.expected,
    hashes: { events: sha256Utf8(eventsText) },
    allowlist: [
      ...proofAllowlist(hostSubjectsRequired(options.captureMode, options.hostIdentity)),
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
  requireFreshProofRoot(options.proofRoot);
  const credential = Array.isArray(options.childArgv)
    ? credentialArgvReason(options.childArgv)
    : null;
  if (credential) {
    return writeProofFiles(options, {
      events: [
        {
          direction: "driver",
          method: "error",
          params: { message: credential },
        },
      ],
      childExit: 1,
      truncated: false,
      streamUnavailable: true,
      stdoutBytes: 0,
      stderrBytes: 0,
    });
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

  if (!options.testOnlyChild && !options.hostIdentity) {
    options.hostIdentity = resolveHostIdentity({ proofRoot: options.proofRoot });
  }
  const bound = options.hostIdentity?.[BOUND_EXEC];
  const spawnOpts = {
    stdio: ["pipe", "pipe", "pipe"],
    env: {
      PATH: process.env.PATH,
      HOME: options.projectRoot,
      CODEX_HOME: path.join(options.projectRoot, ".codex-home"),
    },
  };
  const child = options.testOnlyChild
    ? options.testOnlyChild
    : spawn(bound.codexPath, ["app-server"], spawnOpts);
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
      retainEvent({
        direction: "driver",
        method: "error",
        params: { message: resolved.reason },
      });
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
        if (accepted) {
          acceptedElicitations += 1;
        }
        const reply = {
          id: message.id,
          result: {
            action: accepted ? "accept" : "decline",
            content: {},
          },
        };
        child.stdin.write(encode(reply));
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
      retainEvent({
        direction: "driver",
        method: "error",
        params: { message: "stdio parse failed" },
      });
      stopChild();
    }
  });

  const send = (method, params) => {
    const id = nextId;
    nextId += 1;
    pending.set(id, method);
    retainEvent(projectRetainedEvent({ direction: "client", method, id, params }));
    if (!stopped) {
      child.stdin.write(encode({ id, method, params }));
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
    retainEvent(projectRetainedEvent({ direction: "client", ...initialized }));
    if (!stopped) {
      child.stdin.write(encode(initialized));
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
    retainEvent({ direction: "driver", method: "error", params: { message: String(error) } });
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
