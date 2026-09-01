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
  ALLOWLIST,
  DECIDE_INPUT,
  DECIDE_TOOL,
  EXPECTED_TOOLS,
  HARD_MAX_BYTES,
  HARD_MAX_EVENTS,
  HARD_MAX_FRAMES,
  HARD_MAX_RETAINED_BYTES,
  HARD_MAX_TIMEOUT_MS,
  SCHEMA,
  SKILL_NAME,
  boundedPositiveInt,
  classifyRecord,
  credentialArgvReason,
  decidePrompt,
  driverOutcomeFrom,
  elicitationAcceptable,
  forbiddenProofRoot,
  initializeFromEvents,
  isMainModule,
  persistableArgv,
  requiredCellsForJourney,
  scrub,
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
    provenance: "synthetic",
    timeoutMs: DEFAULT_TIMEOUT_MS,
    maxBytes: DEFAULT_MAX_BYTES,
    journey: "tool",
    allowLiveTurn: false,
    childArgv: null,
    codexBin: null,
    proofRoot: null,
    projectRoot: null,
    assayMcpBin: null,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = () => {
      i += 1;
      return argv[i];
    };
    switch (arg) {
      case "--provenance":
        out.provenance = next();
        break;
      case "--proof-root":
        out.proofRoot = next();
        break;
      case "--project-root":
        out.projectRoot = next();
        break;
      case "--codex-bin":
        out.codexBin = next();
        break;
      case "--assay-mcp-bin":
        out.assayMcpBin = next();
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
  return fs.realpathSync(resolved);
}

export function resolveCodexBin(codexBin) {
  return resolveRegularBinary(codexBin, "codex");
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

function findCodexOnPath() {
  return findOnPath("codex");
}

function probeVersion(bin) {
  const result = spawnSync(bin, ["--version"], {
    encoding: "utf8",
    timeout: 2000,
    maxBuffer: 4096,
  });
  const text = `${result.stdout || ""}${result.stderr || ""}`.trim().split(/\r?\n/)[0] || "";
  if (!text) {
    throw new Error(`could not observe version from ${bin}`);
  }
  return text.slice(0, 200);
}

export function resolveHostIdentity(options) {
  const codexPath = options.codexBin ?? findCodexOnPath();
  const mcpPath = options.assayMcpBin ?? findOnPath("assay-mcp-server");
  if (!codexPath) {
    throw new Error("codex binary was not resolved");
  }
  if (!mcpPath) {
    throw new Error("assay MCP binary was not resolved");
  }
  const codex = resolveRegularBinary(codexPath, "codex");
  const mcp = resolveRegularBinary(mcpPath, "assay MCP");
  return {
    os: os.platform(),
    arch: os.arch(),
    codex: {
      path: codex,
      version: probeVersion(codex),
      sha256: sha256File(codex),
      installSource: options.codexBin ? "codex-bin-flag" : "PATH",
    },
    assayMcp: {
      path: mcp,
      version: probeVersion(mcp),
      sha256: sha256File(mcp),
      installSource: options.assayMcpBin ? "assay-mcp-bin-flag" : "PATH",
    },
  };
}

export function resolveProductionChildArgv(options) {
  const bin = resolveCodexBin(options.codexBin ?? findCodexOnPath());
  return [bin, "app-server"];
}

function resolvedMcpCommand(options) {
  if (options.hostIdentity?.assayMcp?.path) {
    return options.hostIdentity.assayMcp.path;
  }
  if (typeof options.assayMcpBin === "string" && options.assayMcpBin.length > 0) {
    return path.resolve(options.assayMcpBin);
  }
  throw new Error("assay MCP binary was not resolved");
}

function writeProofFiles(options, pack) {
  const initialize = initializeFromEvents(pack.events);
  const invocationArgv = persistableArgv(
    options.hostIdentity?.codex?.path
      ? [options.hostIdentity.codex.path, "app-server"]
      : options.codexBin
        ? [path.resolve(options.codexBin), "app-server"]
        : Array.isArray(options.childArgv)
          ? options.childArgv
          : ["<test-only-child>"],
  );
  const record = {
    schema: SCHEMA,
    provenance: options.provenance,
    journey: options.journey,
    childExitCode: pack.childExit,
    driverOutcome: null,
    truncated: pack.truncated,
    streamUnavailable: pack.streamUnavailable,
    initialize,
    hostIdentity: options.hostIdentity ?? null,
    invocation: { argv: invocationArgv, envNames: ["PATH", "HOME", "CODEX_HOME"] },
    expected: {
      projectRoot: options.projectRoot,
      skillName: SKILL_NAME,
      tools: [...EXPECTED_TOOLS],
      toolName: DECIDE_TOOL,
      toolArguments: DECIDE_INPUT,
    },
    events: pack.events,
  };
  const preliminary = classifyRecord(record);
  let driverOutcome = driverOutcomeFrom(pack, preliminary.cells, options.journey);
  record.driverOutcome = driverOutcome;
  let classified = classifyRecord(record);
  const eventsText = stableStringify(pack.events);
  const manifest = {
    schema: SCHEMA,
    provenance: options.provenance,
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
    hostIdentity: options.hostIdentity ?? null,
    expected: record.expected,
    hashes: { events: sha256Utf8(eventsText) },
    allowlist: [...ALLOWLIST],
  };
  writeJson(path.join(options.proofRoot, "manifest.json"), manifest);
  writeJson(path.join(options.proofRoot, "events.json"), pack.events);
  writeJson(path.join(options.proofRoot, "classification.json"), classified);
  const checked = validateProofRoot(options.proofRoot);
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
  }
  return {
    manifest,
    events: pack.events,
    classified: checked.ok ? checked.classified : classified,
    childExitCode: pack.childExit,
    driverOutcome,
  };
}

export async function runProof(options) {
  requiredCellsForJourney(options.journey);
  const forbidden = forbiddenProofRoot(options.proofRoot, options.provenance);
  if (forbidden) {
    throw new Error(forbidden);
  }
  if (options.provenance === "live" && options.journey === "tool" && !options.allowLiveTurn) {
    throw new Error("live tool journey requires --allow-live-turn; this slice does not authorize a model call");
  }
  if (options.provenance !== "synthetic" && options.provenance !== "live") {
    throw new Error("provenance must be synthetic or live");
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
    : spawn(resolveCodexBin(options.codexBin ?? findCodexOnPath()), ["app-server"], spawnOpts);
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
    if (Object.prototype.hasOwnProperty.call(message, "id") && pending.has(message.id)) {
      const method = pending.get(message.id);
      pending.delete(message.id);
      retainEvent(
        scrub({
          direction: "server",
          method,
          id: message.id,
          result: message.result ?? null,
          error: message.error ?? null,
        }),
      );
      return;
    }
    if (typeof message.method === "string") {
      retainEvent(
        scrub({
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
        const accepted = elicitationAcceptable(message.params, threadId, turnId);
        const reply = {
          id: message.id,
          result: {
            action: accepted ? "accept" : "decline",
            content: {},
          },
        };
        child.stdin.write(encode(reply));
        retainEvent(
          scrub({
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
    retainEvent(scrub({ direction: "client", method, id, params }));
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
    retainEvent({ direction: "client", method: "initialized", params: {} });
    if (!stopped) {
      child.stdin.write(encode({ method: "initialized" }));
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
  options.hostIdentity = resolveHostIdentity(options);
  options.codexBin = options.hostIdentity.codex.path;
  options.assayMcpBin = options.hostIdentity.assayMcp.path;
  return runProof(options);
}

if (isMainModule(process.argv[1], import.meta.url)) {
  main()
    .then((result) => {
      process.stdout.write(stableStringify({
        provenance: result.manifest.provenance,
        childExitCode: result.childExitCode,
        driverOutcome: result.driverOutcome,
        liveAcceptance: result.classified.liveAcceptance,
      }));
      let exitCode = result.driverOutcome.exitCode;
      if (
        result.manifest.provenance === "live" &&
        result.classified.liveAcceptance.status !== "pass" &&
        exitCode === 0
      ) {
        exitCode = 1;
      }
      if (exitCode !== 0) {
        process.exitCode = exitCode;
      }
    })
    .catch((error) => {
      process.stderr.write(`${error.message}\n`);
      process.exitCode = 1;
    });
}
