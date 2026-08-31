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
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  ALLOWLIST,
  DECIDE_INPUT,
  DECIDE_TOOL,
  EXPECTED_TOOLS,
  SCHEMA,
  SKILL_NAME,
  classifyRecord,
  credentialArgvReason,
  decidePrompt,
  finitePositiveInt,
  forbiddenProofRoot,
  persistableArgv,
  scrub,
  sha256Utf8,
  stableStringify,
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
    proofRoot: null,
    projectRoot: null,
    mcpCommand: "/nonexistent/assay-mcp-server",
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
      case "--child-argv":
        out.childArgv = JSON.parse(next());
        break;
      case "--mcp-command":
        out.mcpCommand = next();
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
  const tmp = `${file}.tmp`;
  fs.writeFileSync(tmp, stableStringify(value));
  fs.renameSync(tmp, file);
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

function writeProofFiles(options, pack) {
  const initialize = pack.events.find(
    (event) => event.method === "initialize" && event.direction === "server",
  );
  const record = {
    schema: SCHEMA,
    provenance: options.provenance,
    driverExitCode: pack.childExit,
    truncated: pack.truncated,
    streamUnavailable: pack.streamUnavailable,
    initialize: {
      codexHome: initialize?.result?.codexHome ?? null,
      userAgent: initialize?.result?.userAgent ?? null,
    },
    expected: {
      projectRoot: options.projectRoot,
      skillName: SKILL_NAME,
      tools: [...EXPECTED_TOOLS],
      toolName: DECIDE_TOOL,
      toolArguments: DECIDE_INPUT,
    },
    events: pack.events,
  };
  const classified = classifyRecord(record);
  const eventsText = stableStringify(pack.events);
  const manifest = {
    schema: SCHEMA,
    provenance: options.provenance,
    driverExitCode: pack.childExit,
    truncated: pack.truncated,
    streamUnavailable: pack.streamUnavailable,
    bounds: {
      timeoutMs: options.timeoutMs,
      maxBytes: options.maxBytes,
      stdoutBytes: pack.stdoutBytes,
      stderrBytes: pack.stderrBytes,
    },
    invocation: {
      argv: persistableArgv(options.childArgv),
      envNames: ["PATH", "HOME", "CODEX_HOME"],
    },
    initialize: record.initialize,
    expected: record.expected,
    hashes: { events: sha256Utf8(eventsText) },
    allowlist: [...ALLOWLIST],
  };
  writeJson(path.join(options.proofRoot, "manifest.json"), manifest);
  writeJson(path.join(options.proofRoot, "events.json"), pack.events);
  writeJson(path.join(options.proofRoot, "classification.json"), classified);
  return { manifest, events: pack.events, classified, driverExitCode: pack.childExit };
}

export async function runProof(options) {
  const forbidden = forbiddenProofRoot(options.proofRoot, options.provenance);
  if (forbidden) {
    throw new Error(forbidden);
  }
  if (!Array.isArray(options.childArgv) || options.childArgv.length === 0) {
    throw new Error("child argv is required; this driver does not spawn a default Codex binary");
  }
  if (options.provenance === "live" && options.journey === "tool" && !options.allowLiveTurn) {
    throw new Error("live tool journey requires --allow-live-turn; this slice does not authorize a model call");
  }
  if (options.provenance !== "synthetic" && options.provenance !== "live") {
    throw new Error("provenance must be synthetic or live");
  }
  finitePositiveInt("timeoutMs", options.timeoutMs);
  finitePositiveInt("maxBytes", options.maxBytes);

  fs.mkdirSync(options.proofRoot, { recursive: true, mode: 0o700 });
  const credential = credentialArgvReason(options.childArgv);
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
  let childExit = null;
  const pending = new Map();

  const child = spawn(options.childArgv[0], options.childArgv.slice(1), {
    stdio: ["pipe", "pipe", "pipe"],
    env: {
      PATH: process.env.PATH,
      HOME: options.projectRoot,
      CODEX_HOME: path.join(options.projectRoot, ".codex-home"),
    },
  });
  const childClosed = new Promise((resolve) => {
    child.on("close", (code, signal) => {
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
    const message = parseLine(line, options.maxBytes);
    if (Object.prototype.hasOwnProperty.call(message, "id") && pending.has(message.id)) {
      const method = pending.get(message.id);
      pending.delete(message.id);
      events.push(
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
      events.push(
        scrub({
          direction: "server",
          method: message.method,
          id: message.id ?? null,
          params: message.params ?? null,
        }),
      );
      if (message.method === "mcpServer/elicitation/request" && message.id != null && !stopped) {
        const accept = {
          id: message.id,
          result: { action: "accept", content: {} },
        };
        child.stdin.write(encode(accept));
        events.push(scrub({ direction: "client", method: message.method, id: message.id, result: accept.result }));
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
      events.push({
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
    events.push(scrub({ direction: "client", method, id, params }));
    if (!stopped) {
      child.stdin.write(encode({ id, method, params }));
    }
    return id;
  };

  const blocked = () =>
    stdout.truncated || stderr.truncated || streamUnavailable || stopped;

  const waitFor = async (id) => {
    const deadline = Date.now() + options.timeoutMs;
    while (Date.now() < deadline) {
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

  const waitForExpectedTool = async (threadId, turnId) => {
    const deadline = Date.now() + options.timeoutMs;
    while (Date.now() < deadline) {
      if (blocked()) {
        return;
      }
      const matched = events.some(
        (event) =>
          event.method === "item/completed" &&
          event.direction === "server" &&
          event.params?.threadId === threadId &&
          event.params?.turnId === turnId &&
          event.params?.item?.type === "mcpToolCall",
      );
      if (matched) {
        return;
      }
      const failedTurn = events.some(
        (event) =>
          event.method === "turn/completed" &&
          event.direction === "server" &&
          event.params?.threadId === threadId &&
          event.params?.turn?.id === turnId &&
          event.params?.turn?.status === "failed",
      );
      if (failedTurn) {
        return;
      }
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    throw new Error("timeout waiting for expected tool completion");
  };

  try {
    send("initialize", {
      clientInfo: { name: "assay-codex-host-proof", version: "1" },
      capabilities: {},
    });
    await waitFor(1);
    events.push({ direction: "client", method: "initialized", params: {} });
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
      const primary = startThread(options.mcpCommand, ["--policy-root", "."]);
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
        const invalid = startThread(options.mcpCommand, [
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
        await waitForExpectedTool(threadId, turnId);
      }
    }
  } catch (error) {
    streamUnavailable = streamUnavailable || /timeout|unavailable/i.test(String(error));
    events.push({ direction: "driver", method: "error", params: { message: String(error) } });
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

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main()
    .then((result) => {
      process.stdout.write(stableStringify({
        provenance: result.manifest.provenance,
        driverExitCode: result.driverExitCode,
        liveAcceptance: result.classified.liveAcceptance,
      }));
      if (result.driverExitCode !== 0) {
        process.exitCode = result.driverExitCode;
      }
    })
    .catch((error) => {
      process.stderr.write(`${error.message}\n`);
      process.exitCode = 1;
    });
}
