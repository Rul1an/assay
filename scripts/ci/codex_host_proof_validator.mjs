#!/usr/bin/env node
/**
 * Fail-closed Codex host-proof validator. Owns the single classification
 * function the driver must call. Synthetic records cannot become live proof.
 */
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const SCHEMA = "assay.codex-host-proof.v2";
export const FAKE_USER_AGENT = "assay-codex-host-proof-fake/1";
export const SKILL_NAME = "assay-golden-path";
export const DECIDE_TOOL = "assay_policy_decide";
export const DECIDE_INPUT = {
  tool: "install_surface_allowed_probe",
  policy: "install-surface-policy.yaml",
};
export const ALLOWED_PAYLOAD = {
  allowed: true,
  reason: "Allowed by policy",
};
export const EXPECTED_TOOLS = Object.freeze([
  "assay_check_args",
  "assay_check_sequence",
  "assay_policy_decide",
  "assay_check_coverage",
  "assay_explain_trace",
]);
export const ALLOWLIST = Object.freeze([
  "manifest.json",
  "events.json",
  "classification.json",
]);
export const CELLS = Object.freeze([
  "skillDiscovered",
  "mcpStarted",
  "exactToolsListed",
  "oneToolInvoked",
  "structuredResultValidated",
  "missingBinaryNotClean",
  "invalidPolicyRootNotClean",
  "cwdObserved",
  "driverCompleted",
]);

export function decidePrompt() {
  return `Invoke ${DECIDE_TOOL} with ${JSON.stringify(DECIDE_INPUT)}`;
}

export const HARD_MAX_BYTES = 4 * 1024 * 1024;
export const HARD_MAX_TIMEOUT_MS = 120_000;
export const HARD_MAX_EVENTS = 4096;
export const HARD_MAX_FRAMES = 8192;
export const HARD_MAX_RETAINED_BYTES = 4 * 1024 * 1024;
export const HARD_MAX_DIR_ENTRIES = 64;
// 512 MiB per-binary PATH snapshot ceiling (536870912). Prepared v5.5.2 host
// assets: bundled Codex at /Applications/ChatGPT.app/Contents/Resources/codex
// is 231,697,328 bytes; assay-mcp-server is 11,105,184 bytes. 256 MiB would
// sit one routine host growth away from false-unavailable.
export const HARD_MAX_SNAPSHOT_BYTES = 512 * 1024 * 1024;
export const LIVE_IDENTITY_NONCLAIM =
  "event-derived and internally consistent identity is not authenticated or tamper-evident";

export function finitePositiveInt(name, value) {
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${name} must be a finite positive integer`);
  }
  return value;
}

export function boundedPositiveInt(name, value, hardMax) {
  const n = finitePositiveInt(name, value);
  if (n > hardMax) {
    throw new Error(`${name} exceeds hard maximum ${hardMax}`);
  }
  return n;
}

const CREDENTIAL_ARGV_NAME =
  /^--(api-key|token|authorization|password|secret)$/i;

export function normalizeOptionName(arg) {
  if (typeof arg !== "string") {
    return "";
  }
  const raw = arg.includes("=") ? arg.slice(0, arg.indexOf("=")) : arg;
  if (!raw.startsWith("-")) {
    return raw;
  }
  return raw.toLowerCase().replaceAll("_", "-");
}

function isCredentialOption(arg) {
  return CREDENTIAL_ARGV_NAME.test(normalizeOptionName(arg));
}

export function credentialArgvReason(argv) {
  for (const arg of argv) {
    if (typeof arg !== "string") {
      continue;
    }
    if (isCredentialOption(arg)) {
      return "credential-bearing argv is rejected before spawn";
    }
  }
  return null;
}

export function persistableArgv(argv) {
  const out = [];
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (typeof arg !== "string") {
      out.push(arg);
      continue;
    }
    if (arg.includes("=")) {
      const name = arg.slice(0, arg.indexOf("="));
      out.push(isCredentialOption(arg) ? `${name}=[redacted]` : arg);
      continue;
    }
    if (isCredentialOption(arg)) {
      out.push(arg);
      if (i + 1 < argv.length) {
        i += 1;
        out.push("[redacted]");
      }
      continue;
    }
    out.push(arg);
  }
  return out;
}

function emptyInitialize() {
  return {
    codexHome: null,
    userAgent: null,
    platformFamily: null,
    platformOs: null,
  };
}

export function successfulInitializeResult(result) {
  return (
    result != null &&
    typeof result === "object" &&
    !Array.isArray(result) &&
    typeof result.userAgent === "string" &&
    result.userAgent.length > 0 &&
    typeof result.codexHome === "string" &&
    result.codexHome.length > 0 &&
    typeof result.platformFamily === "string" &&
    result.platformFamily.length > 0 &&
    typeof result.platformOs === "string" &&
    result.platformOs.length > 0
  );
}

export function initializeFromTopology(topology) {
  const pairs = Array.isArray(topology?.pairs)
    ? topology.pairs.filter((pair) => pair.method === "initialize")
    : [];
  if (pairs.length !== 1) {
    return emptyInitialize();
  }
  const response = pairs[0].response;
  if (
    response == null ||
    Object.prototype.hasOwnProperty.call(response, "error") ||
    !successfulInitializeResult(response.result)
  ) {
    return emptyInitialize();
  }
  return {
    codexHome: response.result.codexHome,
    userAgent: response.result.userAgent,
    platformFamily: response.result.platformFamily,
    platformOs: response.result.platformOs,
  };
}

export function initializeFromEvents(events, journey = "tool") {
  if (!Array.isArray(events)) {
    return emptyInitialize();
  }
  try {
    return initializeFromTopology(consumeJourneyTopology(events, journey));
  } catch {
    return emptyInitialize();
  }
}

export function pathInsideRoot(root, candidate) {
  if (typeof root !== "string" || typeof candidate !== "string") {
    return false;
  }
  const rel = path.relative(path.resolve(root), path.resolve(candidate));
  return rel !== "" && !rel.startsWith("..") && !path.isAbsolute(rel);
}

function boundBinary(bin) {
  return (
    Boolean(bin) &&
    typeof bin === "object" &&
    typeof bin.path === "string" &&
    path.isAbsolute(bin.path) &&
    typeof bin.version === "string" &&
    bin.version.length > 0 &&
    typeof bin.sha256 === "string" &&
    /^[a-f0-9]{64}$/.test(bin.sha256) &&
    typeof bin.installSource === "string" &&
    bin.installSource.length > 0
  );
}

export function liveIdentityBound(identity) {
  if (!identity || typeof identity !== "object") {
    return false;
  }
  if (typeof identity.os !== "string" || identity.os.length === 0) {
    return false;
  }
  if (typeof identity.arch !== "string" || identity.arch.length === 0) {
    return false;
  }
  return boundBinary(identity.codex) && boundBinary(identity.assayMcp);
}

export function consumeBoundedBinary(file, maxBytes, onChunk, options = {}) {
  const ceiling = maxBytes ?? HARD_MAX_SNAPSHOT_BYTES;
  const flags = fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0);
  const fd = fs.openSync(file, flags);
  try {
    const st = fs.fstatSync(fd);
    if (!st.isFile()) {
      throw new Error("bounded binary is not a regular file");
    }
    if (st.size > ceiling) {
      throw new Error("bounded binary exceeds binary ceiling");
    }
    if (typeof options.afterStat === "function") {
      options.afterStat(st);
    }
    const buf = Buffer.alloc(64 * 1024);
    let copied = 0;
    let n;
    while ((n = fs.readSync(fd, buf, 0, buf.length, null)) > 0) {
      if (typeof options.testOnlyAfterRead === "function") {
        options.testOnlyAfterRead(copied, file);
      }
      copied += n;
      if (copied > ceiling) {
        throw new Error("bounded binary copy exceeded binary ceiling");
      }
      if (fs.fstatSync(fd).size > ceiling) {
        throw new Error("bounded binary grew past binary ceiling");
      }
      onChunk(buf.subarray(0, n));
    }
    return copied;
  } finally {
    fs.closeSync(fd);
  }
}

export function sha256File(file, options = {}) {
  const hash = createHash("sha256");
  consumeBoundedBinary(
    file,
    options.maxBytes ?? HARD_MAX_SNAPSHOT_BYTES,
    (chunk) => {
      hash.update(chunk);
    },
    options,
  );
  return hash.digest("hex");
}

function verifyObservedBinary(bin) {
  try {
    const st = fs.lstatSync(bin.path);
    if (st.isSymbolicLink() || !st.isFile()) {
      return false;
    }
    return sha256File(bin.path) === bin.sha256;
  } catch {
    return false;
  }
}

export function verifyLiveIdentity(identity, invocation, topology) {
  if (!liveIdentityBound(identity)) {
    return false;
  }
  if (!verifyObservedBinary(identity.codex) || !verifyObservedBinary(identity.assayMcp)) {
    return false;
  }
  const command =
    topology?.primaryThread?.request?.params?.config?.mcp_servers?.assay?.command;
  if (command !== identity.assayMcp.path) {
    return false;
  }
  const argv = invocation?.argv;
  return Array.isArray(argv) && argv[0] === identity.codex.path && argv[1] === "app-server";
}

export function observedIdentityBound(identity, invocation, topology) {
  return verifyLiveIdentity(identity, invocation, topology);
}

export function requiredCellsForJourney(journey) {
  switch (journey) {
    case "discovery":
      return ["skillDiscovered"];
    case "failures":
      return CELLS.filter(
        (name) =>
          name !== "driverCompleted" &&
          name !== "oneToolInvoked" &&
          name !== "structuredResultValidated",
      );
    case "tool":
      return CELLS.filter((name) => name !== "driverCompleted");
    default:
      throw new Error(`unknown journey ${journey}`);
  }
}

export const EXPECTED_ELICITATION = Object.freeze({
  serverName: "assay",
  mode: "form",
  message: `approve ${DECIDE_TOOL}`,
  requestedSchema: Object.freeze({
    type: "object",
    properties: Object.freeze({}),
  }),
});

export function elicitationAcceptable(params, threadId, turnId) {
  return (
    params != null &&
    typeof params === "object" &&
    params.serverName === EXPECTED_ELICITATION.serverName &&
    params.mode === EXPECTED_ELICITATION.mode &&
    params.message === EXPECTED_ELICITATION.message &&
    sameJson(params.requestedSchema, EXPECTED_ELICITATION.requestedSchema) &&
    typeof threadId === "string" &&
    threadId.length > 0 &&
    typeof turnId === "string" &&
    turnId.length > 0 &&
    params.threadId === threadId &&
    params.turnId === turnId
  );
}

export function driverOutcomeExit(pack, cells, journey) {
  const child = pack.childExit;
  const fail = child && child !== 0 ? child : 1;
  if (pack.truncated || pack.streamUnavailable) {
    return fail;
  }
  if (cells?.driverCompleted?.status === "unavailable") {
    return fail;
  }
  const required = requiredCellsForJourney(journey);
  for (const name of required) {
    if (cells?.[name]?.status !== "pass") {
      return fail;
    }
  }
  if (child !== 0) {
    return fail;
  }
  return 0;
}

export function closedDriverOutcomeStatus(meta) {
  if (meta.streamUnavailable || meta.truncated) {
    return "unavailable";
  }
  const child = meta.childExitCode;
  const outcome = meta.driverOutcome;
  if (outcome == null) {
    return child === 0 ? "preliminary" : "fail";
  }
  if (typeof outcome !== "object" || Array.isArray(outcome) || typeof outcome.exitCode !== "number") {
    return "invalid";
  }
  if (child === 0 && outcome.exitCode === 0 && outcome.status === "pass") {
    return "pass";
  }
  const nonzero = child !== 0 || outcome.exitCode !== 0;
  if (nonzero && outcome.status === "fail") {
    return "fail";
  }
  return "invalid";
}

export function driverOutcomeFrom(pack, cells, journey) {
  const exitCode = driverOutcomeExit(pack, cells, journey);
  let status = "pass";
  if (pack.truncated || pack.streamUnavailable) {
    status = "unavailable";
  } else if (exitCode !== 0) {
    status = "fail";
  }
  const draft = { exitCode, status };
  const kind = closedDriverOutcomeStatus({
    childExitCode: pack.childExit,
    driverOutcome: draft,
    truncated: pack.truncated,
    streamUnavailable: pack.streamUnavailable,
  });
  if (kind === "invalid") {
    return { exitCode: exitCode === 0 ? 1 : exitCode, status: "fail" };
  }
  return draft;
}

export function resolvePendingResponse(pending, frame) {
  if (!frame || typeof frame !== "object" || Array.isArray(frame)) {
    return { kind: "reject", reason: "rpc frame is not an object" };
  }
  const hasId = Object.prototype.hasOwnProperty.call(frame, "id");
  if (hasId && !isProofRpcId(frame.id)) {
    return { kind: "reject", reason: "invalid retained-proof rpc id" };
  }
  const hasMethod = Object.prototype.hasOwnProperty.call(frame, "method");
  const hasResult = Object.prototype.hasOwnProperty.call(frame, "result");
  const hasError = Object.prototype.hasOwnProperty.call(frame, "error");
  if (hasId && !hasMethod && (hasResult || hasError)) {
    if (hasResult === hasError) {
      return { kind: "reject", reason: "response must have exactly one of result or error" };
    }
    if (!pending.has(frame.id)) {
      return { kind: "reject", reason: "unknown response id" };
    }
    const method = pending.get(frame.id);
    pending.delete(frame.id);
    return {
      kind: "resolve",
      method,
      id: frame.id,
      result: hasResult ? frame.result : undefined,
      error: hasError ? frame.error : undefined,
    };
  }
  if (hasId && !hasMethod && !hasResult && !hasError) {
    return { kind: "reject", reason: "malformed response envelope" };
  }
  if (hasId && hasMethod && (hasResult || hasError)) {
    return { kind: "reject", reason: "notification cannot resolve a pending id" };
  }
  return { kind: "skip" };
}

export function journeyPairCounts(journey) {
  switch (journey) {
    case "discovery":
      return Object.freeze({
        initialize: 1,
        "skills/list": 1,
        "thread/start": 0,
        "mcpServerStatus/list": 0,
        "turn/start": 0,
      });
    case "failures":
      return Object.freeze({
        initialize: 1,
        "skills/list": 1,
        "thread/start": 3,
        "mcpServerStatus/list": 3,
        "turn/start": 0,
      });
    case "tool":
      return Object.freeze({
        initialize: 1,
        "skills/list": 1,
        "thread/start": 3,
        "mcpServerStatus/list": 3,
        "turn/start": 1,
      });
    default:
      throw new Error(`unknown journey ${journey}`);
  }
}

export const ALLOWED_SERVER_REQUESTS = Object.freeze(["mcpServer/elicitation/request"]);
export const ALLOWED_SERVER_NOTIFICATIONS = Object.freeze(["item/completed", "turn/completed"]);
export const ALLOWED_CLIENT_NOTIFICATIONS = Object.freeze(["initialized"]);
export const ALLOWED_CLIENT_RESPONSES = Object.freeze(["mcpServer/elicitation/request"]);
export const ALLOWED_DRIVER_METHODS = Object.freeze(["error"]);

function hasOwn(value, key) {
  return value != null && Object.prototype.hasOwnProperty.call(value, key);
}

function isNonemptyString(value) {
  return typeof value === "string" && value.length > 0;
}

function isPlainObject(value) {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

// Retained-proof ID domain only. Not a general JSON-RPC uniqueness rule.
export function isProofRpcId(id) {
  if (typeof id === "string") {
    return id.length > 0;
  }
  return Number.isSafeInteger(id);
}

function retainedItemReason(item) {
  if (!isPlainObject(item) || !isNonemptyString(item.type) || !isNonemptyString(item.id)) {
    return "item/completed item is not a typed retained item";
  }
  switch (item.type) {
    case "mcpToolCall":
      if (
        isNonemptyString(item.server) &&
        isNonemptyString(item.tool) &&
        isPlainObject(item.arguments) &&
        isNonemptyString(item.status) &&
        (item.result == null || isPlainObject(item.result))
      ) {
        return null;
      }
      return "item/completed mcpToolCall is missing required retained fields";
    case "userMessage":
      if (Array.isArray(item.content)) {
        return null;
      }
      return "item/completed userMessage is missing required retained fields";
    default:
      return `unknown retained item type ${item.type}`;
  }
}

function retainedMethodParamsReason(method, params) {
  switch (method) {
    case "item/completed": {
      if (!isPlainObject(params)) {
        return "item/completed params must be an object";
      }
      if (!Number.isFinite(params.completedAtMs)) {
        return "item/completed completedAtMs must be finite";
      }
      if (!isNonemptyString(params.threadId) || !isNonemptyString(params.turnId)) {
        return "item/completed threadId and turnId must be nonempty strings";
      }
      return retainedItemReason(params.item);
    }
    case "turn/completed": {
      if (!isPlainObject(params)) {
        return "turn/completed params must be an object";
      }
      if (!isNonemptyString(params.threadId)) {
        return "turn/completed threadId must be a nonempty string";
      }
      const turn = params.turn;
      if (
        !isPlainObject(turn) ||
        !isNonemptyString(turn.id) ||
        !Array.isArray(turn.items) ||
        !isNonemptyString(turn.status)
      ) {
        return "turn/completed turn must have typed id, items, and status";
      }
      return null;
    }
    case "mcpServer/elicitation/request": {
      if (!isPlainObject(params)) {
        return "elicitation params must be an object";
      }
      if (
        !isNonemptyString(params.serverName) ||
        !isNonemptyString(params.threadId) ||
        !isNonemptyString(params.turnId) ||
        !isNonemptyString(params.message) ||
        !isNonemptyString(params.mode) ||
        !isPlainObject(params.requestedSchema)
      ) {
        return "elicitation params must have typed schema, message, thread, and turn";
      }
      return null;
    }
    default:
      return null;
  }
}

export function classifyStoredEvent(event) {
  if (event == null || typeof event !== "object" || Array.isArray(event)) {
    return { type: "unclassified", reason: "event is not an object" };
  }
  const method = typeof event.method === "string" && event.method.length > 0 ? event.method : null;
  const hasValidId = isProofRpcId(event.id);
  const hasResult = hasOwn(event, "result");
  const hasError = hasOwn(event, "error");
  switch (event.direction) {
    case "driver":
      if (method && ALLOWED_DRIVER_METHODS.includes(method) && !hasResult && !hasError) {
        return { type: "driver", method };
      }
      return { type: "unclassified", reason: "unclassified driver event" };
    case "client":
      if (hasValidId && method && !hasResult && !hasError) {
        return { type: "client-request", method, id: event.id };
      }
      if (
        !hasValidId &&
        method &&
        ALLOWED_CLIENT_NOTIFICATIONS.includes(method) &&
        !hasResult &&
        !hasError
      ) {
        return { type: "client-notification", method };
      }
      if (
        hasValidId &&
        method &&
        ALLOWED_CLIENT_RESPONSES.includes(method) &&
        hasResult &&
        !hasError
      ) {
        return { type: "client-response", method, id: event.id };
      }
      return { type: "unclassified", reason: "unclassified client event" };
    case "server":
      if (hasResult && hasError) {
        return { type: "unclassified", reason: "mixed request/response shape" };
      }
      if (hasValidId && (hasResult || hasError)) {
        return { type: "server-response", method, id: event.id };
      }
      if (!hasValidId && (hasResult || hasError)) {
        return { type: "unclassified", reason: "response fields without valid paired id" };
      }
      if (
        hasValidId &&
        method &&
        !hasResult &&
        !hasError &&
        ALLOWED_SERVER_REQUESTS.includes(method)
      ) {
        const payloadReason = retainedMethodParamsReason(method, event.params);
        if (payloadReason) {
          return { type: "unclassified", reason: payloadReason };
        }
        return { type: "server-request", method, id: event.id };
      }
      if (
        !hasValidId &&
        method &&
        !hasResult &&
        !hasError &&
        ALLOWED_SERVER_NOTIFICATIONS.includes(method)
      ) {
        const payloadReason = retainedMethodParamsReason(method, event.params);
        if (payloadReason) {
          return { type: "unclassified", reason: payloadReason };
        }
        return { type: "server-notification", method };
      }
      return { type: "unclassified", reason: "unclassified server event" };
    default:
      return { type: "unclassified", reason: "unknown event direction" };
  }
}

function threadRoleFromStartParams(params) {
  const assay = params?.config?.mcp_servers?.assay;
  const command = assay?.command ?? "";
  const args = assay?.args ?? [];
  if (String(command).includes("missing-assay-mcp-server")) {
    return "missing";
  }
  if (args.some((value) => String(value).includes("missing-policy-root"))) {
    return "invalid";
  }
  return "primary";
}

function rememberRequestId(ctx, id) {
  if (!isProofRpcId(id)) {
    ctx.reasons.push("invalid retained-proof rpc id");
    return false;
  }
  if (ctx.seen.has(id)) {
    ctx.reasons.push(`reused request id ${id}`);
    return false;
  }
  ctx.seen.add(id);
  return true;
}

function consumeClassifiedEvent(classified, event, ctx) {
  switch (classified.type) {
    case "client-request":
      if (!rememberRequestId(ctx, event.id)) {
        return;
      }
      ctx.pending.set(event.id, event.method);
      ctx.clientById.set(event.id, event);
      return;
    case "server-response": {
      const expectedMethod = ctx.pending.get(event.id);
      if (expectedMethod != null && event.method !== expectedMethod) {
        ctx.reasons.push("notification cannot resolve a pending id");
        return;
      }
      const frame = { id: event.id };
      if (hasOwn(event, "result")) {
        frame.result = event.result;
      }
      if (hasOwn(event, "error")) {
        frame.error = event.error;
      }
      const resolved = resolvePendingResponse(ctx.pending, frame);
      if (resolved.kind === "reject") {
        ctx.reasons.push(resolved.reason);
        return;
      }
      if (resolved.kind === "resolve") {
        ctx.pairs.push({
          method: resolved.method,
          id: resolved.id,
          request: ctx.clientById.get(resolved.id),
          response: event,
        });
      }
      return;
    }
    case "server-request":
      if (!rememberRequestId(ctx, event.id)) {
        return;
      }
      ctx.pendingServer.set(event.id, event.method);
      ctx.serverRequests.push(event);
      return;
    case "server-notification":
      ctx.notifications.push(event);
      return;
    case "client-notification":
      return;
    case "client-response": {
      const expected = ctx.pendingServer.get(event.id);
      if (expected == null) {
        ctx.reasons.push("unknown client response id");
        return;
      }
      if (event.method !== expected) {
        ctx.reasons.push("client response method does not match pending server request");
        return;
      }
      ctx.pendingServer.delete(event.id);
      ctx.clientResponses.push(event);
      return;
    }
    case "driver":
      return;
    case "unclassified":
      ctx.reasons.push(classified.reason);
      return;
    default: {
      const unexpected = classified.type;
      ctx.reasons.push(`unknown event class ${unexpected}`);
    }
  }
}

export function consumeJourneyTopology(events, journey) {
  const counts = journeyPairCounts(journey);
  const pending = new Map();
  const pendingServer = new Map();
  const seen = new Set();
  const clientById = new Map();
  const pairs = [];
  const reasons = [];
  const notifications = [];
  const serverRequests = [];
  const clientResponses = [];
  if (!Array.isArray(events)) {
    return { ok: false, reasons: ["events must be an array"], pairs, counts };
  }
  const ctx = {
    pending,
    pendingServer,
    seen,
    clientById,
    pairs,
    reasons,
    notifications,
    serverRequests,
    clientResponses,
  };
  for (const event of events) {
    consumeClassifiedEvent(classifyStoredEvent(event), event, ctx);
  }
  if (pending.size > 0) {
    reasons.push("unresolved client requests");
  }
  if (pendingServer.size > 0) {
    reasons.push("unresolved server requests");
  }
  const byMethod = (method) => pairs.filter((pair) => pair.method === method);
  for (const [method, n] of Object.entries(counts)) {
    const found = byMethod(method).length;
    if (found !== n) {
      reasons.push(`expected ${n} ${method} pairs, found ${found}`);
    }
  }
  for (const pair of pairs) {
    if (!Object.prototype.hasOwnProperty.call(counts, pair.method)) {
      reasons.push(`unexpected ${pair.method} pair`);
    }
  }
  const skillPairs = byMethod("skills/list");
  if (skillPairs.length === 1) {
    const data = skillPairs[0].response.result?.data;
    if (!Array.isArray(data) || data.length !== 1) {
      reasons.push("skills/list must have exactly one data row");
    }
  }
  const roles = new Map();
  for (const pair of byMethod("thread/start")) {
    const role = threadRoleFromStartParams(pair.request?.params);
    if (roles.has(role)) {
      reasons.push(`duplicate ${role} thread/start`);
    }
    roles.set(role, pair);
  }
  if (counts["thread/start"] === 3) {
    for (const role of ["primary", "missing", "invalid"]) {
      if (!roles.has(role)) {
        reasons.push(`missing ${role} thread/start`);
      }
    }
  }
  const statusByRole = new Map();
  for (const pair of byMethod("mcpServerStatus/list")) {
    const threadId = pair.request?.params?.threadId;
    let role = null;
    for (const [name, thread] of roles) {
      if (thread.response.result?.thread?.id === threadId) {
        role = name;
        break;
      }
    }
    if (!role) {
      reasons.push("status list is not bound to a thread/start pair");
      continue;
    }
    if (statusByRole.has(role)) {
      reasons.push(`duplicate ${role} status list`);
    }
    statusByRole.set(role, pair);
  }
  if (counts["mcpServerStatus/list"] === 3) {
    for (const role of ["primary", "missing", "invalid"]) {
      if (!statusByRole.has(role)) {
        reasons.push(`missing ${role} status list`);
      }
    }
  }
  const turnPairs = byMethod("turn/start");
  if (counts["turn/start"] === 1 && turnPairs.length === 1) {
    const primaryId = roles.get("primary")?.response?.result?.thread?.id;
    if (turnPairs[0].request?.params?.threadId !== primaryId) {
      reasons.push("turn/start is not bound to the primary thread");
    }
  }
  const initializePairs = byMethod("initialize");
  if (counts.initialize === 1) {
    if (
      initializePairs.length !== 1 ||
      hasOwn(initializePairs[0].response, "error") ||
      !successfulInitializeResult(initializePairs[0].response?.result)
    ) {
      reasons.push("initialize must be one successful result with required live fields");
    }
  }
  return {
    ok: reasons.length === 0,
    reasons,
    pairs,
    counts,
    primaryThread: roles.get("primary") ?? null,
    threads: ["primary", "missing", "invalid"]
      .map((role) => roles.get(role)?.response)
      .filter(Boolean),
    statuses: ["primary", "missing", "invalid"]
      .map((role) => statusByRole.get(role)?.response)
      .filter(Boolean),
    skills: skillPairs.map((pair) => pair.response),
    notifications,
    serverRequests,
    clientResponses,
  };
}

export function isMainModule(argv1, moduleUrl) {
  if (typeof argv1 !== "string" || argv1.length === 0) {
    return false;
  }
  const modulePath = fileURLToPath(moduleUrl);
  try {
    return fs.realpathSync(path.resolve(argv1)) === fs.realpathSync(modulePath);
  } catch {
    return path.resolve(argv1) === path.resolve(modulePath);
  }
}

const STATUSES = new Set(["pass", "fail", "unavailable", "not_applicable"]);
const MAX_DEPTH = 32;
const CREDENTIAL_KEY =
  /pass(word)?|secret|token|authorization|api[_-]?key|auth\.json|sk-[a-z0-9]/i;

export function sha256Utf8(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

export function stableStringify(value) {
  return `${stringifySorted(value)}\n`;
}

function stringifySorted(value) {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(stringifySorted).join(",")}]`;
  }
  const keys = Object.keys(value).sort();
  return `{${keys
    .map((key) => `${JSON.stringify(key)}:${stringifySorted(value[key])}`)
    .join(",")}}`;
}

export function walkDepth(value, depth = 0) {
  if (depth > MAX_DEPTH) {
    throw new Error("json depth exceeds bound");
  }
  if (value === null || typeof value !== "object") {
    return;
  }
  const children = Array.isArray(value) ? value : Object.values(value);
  for (const child of children) {
    walkDepth(child, depth + 1);
  }
}

export function parseJsonBytes(bytes, limit) {
  if (!Buffer.isBuffer(bytes)) {
    bytes = Buffer.from(bytes);
  }
  if (bytes.length > limit) {
    throw new Error("input exceeds byte bound");
  }
  const text = bytes.toString("utf8");
  if (text.includes("\u0000")) {
    throw new Error("nul byte in json");
  }
  const value = JSON.parse(text);
  walkDepth(value);
  return value;
}

export function cell(status, reason) {
  if (!STATUSES.has(status)) {
    throw new Error(`invalid status ${status}`);
  }
  return { status, reason };
}

function skillsFromEvents(events) {
  const listed = [];
  for (const event of events) {
    if (event.method !== "skills/list" || event.direction !== "server") {
      continue;
    }
    const data = event.result?.data;
    if (!Array.isArray(data)) {
      continue;
    }
    for (const entry of data) {
      if (Array.isArray(entry?.skills)) {
        listed.push(...entry.skills);
      }
    }
  }
  return listed;
}

function mcpToolCalls(rows) {
  const calls = [];
  for (const event of rows) {
    if (event.method !== "item/completed" || event.direction !== "server") {
      continue;
    }
    const item = event.params?.item;
    if (item?.type === "mcpToolCall") {
      calls.push({
        item,
        threadId: event.params?.threadId,
        turnId: event.params?.turnId,
      });
    }
  }
  return calls;
}

function primaryThreadId(starts) {
  const id = starts[0]?.result?.thread?.id;
  return typeof id === "string" && id.length > 0 ? id : null;
}

function turnIdFromTopology(topology) {
  const pairs = (topology.pairs || []).filter((pair) => pair.method === "turn/start");
  if (pairs.length !== 1) {
    return null;
  }
  const id = pairs[0].response?.result?.turn?.id;
  return typeof id === "string" && id.length > 0 ? id : null;
}

function toolNames(status) {
  const tools = status?.tools;
  if (!tools || typeof tools !== "object" || Array.isArray(tools)) {
    return [];
  }
  return Object.keys(tools).sort();
}

function payloadFromCall(call) {
  const structured = call?.result?.structuredContent;
  if (structured && typeof structured === "object") {
    return structured;
  }
  const content = call?.result?.content;
  if (!Array.isArray(content)) {
    return null;
  }
  const texts = content
    .filter((block) => block && typeof block.text === "string")
    .map((block) => block.text);
  if (texts.length !== 1) {
    return null;
  }
  try {
    return JSON.parse(texts[0]);
  } catch {
    return null;
  }
}

function sameJson(left, right) {
  return stableStringify(left) === stableStringify(right);
}

function classifySkill(skills, expected) {
  const found = skills.filter((skill) => skill?.name === expected.skillName);
  if (found.length === 0) {
    return cell("fail", `missing skill ${expected.skillName}`);
  }
  if (found.length !== 1) {
    return cell("fail", "duplicate or contradictory skill rows");
  }
  const enabled = found[0];
  if (
    enabled.enabled !== true ||
    typeof enabled.path !== "string" ||
    !pathInsideRoot(expected.projectRoot, enabled.path)
  ) {
    return cell("fail", "skill present but not enabled under project root");
  }
  return cell("pass", `discovered ${expected.skillName}`);
}

function classifyCwd(starts, expected) {
  const primary = starts[0];
  const cwd = primary?.result?.cwd;
  if (typeof cwd !== "string") {
    return cell("unavailable", "thread/start cwd absent");
  }
  if (cwd !== expected.projectRoot) {
    return cell("fail", `cwd ${cwd} is not project root`);
  }
  return cell("pass", "thread cwd is project root");
}

function uniqueAssayRow(data) {
  if (!Array.isArray(data)) {
    return null;
  }
  const rows = data.filter((row) => row?.name === "assay");
  return rows.length === 1 ? rows[0] : null;
}

function classifyMcp(statuses) {
  const assay = uniqueAssayRow(statuses[0]?.result?.data);
  if (!assay) {
    return cell("unavailable", "no assay mcpServerStatus row");
  }
  if (assay.runtimeStatus !== "connected") {
    return cell("fail", `runtimeStatus ${assay.runtimeStatus}`);
  }
  return cell("pass", "assay MCP connected");
}

function classifyTools(statuses) {
  const assay = uniqueAssayRow(statuses[0]?.result?.data);
  if (!assay) {
    return cell("unavailable", "no assay tools row");
  }
  const names = toolNames(assay);
  const expected = [...EXPECTED_TOOLS].sort();
  if (stableStringify(names) !== stableStringify(expected)) {
    return cell("fail", `tools ${names.join(",")} != ${expected.join(",")}`);
  }
  return cell("pass", "exact release tools listed");
}

// Notification/elicitation consumers read topology collections only.
// They must not rescan raw events for initialize/thread/status/skills.
function matchingTurnStatus(rows, threadId, turnId) {
  const matches = rows.filter(
    (row) =>
      row.method === "turn/completed" &&
      row.direction === "server" &&
      row.params?.threadId === threadId &&
      row.params?.turn?.id === turnId,
  );
  if (matches.length === 0) {
    return null;
  }
  if (matches.length !== 1) {
    return "non-unique";
  }
  const status = matches[0].params?.turn?.status;
  return typeof status === "string" ? status : "";
}

function uniqueExpectedElicitationAccepted(topology, threadId, turnId) {
  const requests = Array.isArray(topology?.serverRequests) ? topology.serverRequests : [];
  const replies = Array.isArray(topology?.clientResponses) ? topology.clientResponses : [];
  const acceptable = requests.filter((event) =>
    elicitationAcceptable(event.params, threadId, turnId),
  );
  if (acceptable.length !== 1 || acceptable[0].id == null) {
    return false;
  }
  const requestId = acceptable[0].id;
  const matchingAccepts = replies.filter(
    (event) => event.id === requestId && event.result?.action === "accept",
  );
  if (matchingAccepts.length !== 1) {
    return false;
  }
  const allAccepts = replies.filter((event) => event.result?.action === "accept");
  return allAccepts.length === 1;
}

function classifyInvocation(calls, expected, threadId, turnId, topology) {
  if (calls.length === 0) {
    return cell("unavailable", "no mcpToolCall item/completed");
  }
  const terminalStatus = matchingTurnStatus(topology?.notifications ?? [], threadId, turnId);
  if (terminalStatus === null) {
    return cell("unavailable", "no matching turn/completed");
  }
  if (terminalStatus !== "completed") {
    return cell("fail", `matching turn/completed status ${terminalStatus} is not completed`);
  }
  if (typeof threadId !== "string" || typeof turnId !== "string") {
    return cell("unavailable", "primary thread or turn id absent");
  }
  if (calls.length !== 1) {
    return cell(
      "fail",
      `expected one mcpToolCall, found ${calls.length}`,
    );
  }
  const call = calls[0];
  const item = call.item;
  if (item.server !== "assay") {
    return cell("fail", `wrong server ${item.server}`);
  }
  if (call.threadId !== threadId) {
    return cell("fail", `tool thread ${call.threadId} != ${threadId}`);
  }
  if (call.turnId !== turnId) {
    return cell("fail", `tool turn ${call.turnId} != ${turnId}`);
  }
  if (!EXPECTED_TOOLS.includes(item.tool)) {
    return cell("fail", `tool ${item.tool} is not in the listed release set`);
  }
  if (item.tool !== DECIDE_TOOL) {
    return cell("fail", `wrong tool ${item.tool}`);
  }
  if (item.status !== "completed") {
    return cell("fail", `tool status ${item.status}`);
  }
  if (!sameJson(item.arguments, DECIDE_INPUT)) {
    return cell("fail", "tool arguments are not the pinned probe");
  }
  if (!uniqueExpectedElicitationAccepted(topology, threadId, turnId)) {
    return cell(
      "fail",
      "oneToolInvoked requires exactly one expected elicitation request and one matching accept",
    );
  }
  return cell("pass", `invoked ${expected.toolName}`);
}

function classifyPayload(calls, invocationCell) {
  if (invocationCell.status !== "pass") {
    return cell("unavailable", "invocation cell did not pass");
  }
  const result = calls[0].item?.result;
  if (result && typeof result === "object") {
    if (result["isError"] === true) {
      return cell("fail", "MCP result isError is true");
    }
    if (result.error != null) {
      return cell("fail", "MCP result is error-bearing");
    }
  }
  const payload = payloadFromCall(calls[0].item);
  if (!payload) {
    return cell("fail", "tool result is not typed JSON");
  }
  if (!sameJson(payload, ALLOWED_PAYLOAD)) {
    return cell("fail", "typed result is not the pinned allow payload");
  }
  return cell("pass", "typed JSON allow payload");
}

function classifyNegative(statuses, index, label) {
  const assay = uniqueAssayRow(statuses[index]?.result?.data);
  if (!assay) {
    return cell("unavailable", `${label}: no assay status`);
  }
  const names = toolNames(assay);
  const clean =
    assay.runtimeStatus === "connected" && names.length > 0;
  if (clean) {
    return cell("fail", `${label} presented a clean connected tool list`);
  }
  if (assay.runtimeStatus === "failed" && names.length === 0) {
    return cell("pass", `${label} failed with zero tools`);
  }
  return cell(
    "fail",
    `${label} status ${assay.runtimeStatus} tools=${names.length} is not a host failure`,
  );
}

function classifyDriver(meta) {
  const kind = closedDriverOutcomeStatus(meta);
  if (kind === "unavailable") {
    if (meta.streamUnavailable) {
      return cell("unavailable", "stdio stream unavailable");
    }
    if (meta.truncated) {
      return cell("unavailable", "stdio truncated at bound");
    }
    return cell("unavailable", "driver outcome unavailable");
  }
  if (kind === "pass") {
    return cell("pass", "child and driver exits 0 with status pass");
  }
  if (kind === "preliminary") {
    return cell("pass", "child exited 0 pending driver outcome");
  }
  if (kind === "fail") {
    const exit = meta.driverOutcome?.exitCode ?? meta.childExitCode;
    return cell("fail", `driver outcome exit ${exit}`);
  }
  return cell("fail", "contradictory or malformed driver outcome");
}

export function classifyCells(events, meta, expected, journey = "tool", topology = null) {
  if (meta.streamUnavailable || meta.truncated) {
    const blocked = classifyDriver(meta);
    const rest = Object.fromEntries(
      CELLS.filter((name) => name !== "driverCompleted").map((name) => [
        name,
        cell("unavailable", blocked.reason),
      ]),
    );
    return { ...rest, driverCompleted: blocked };
  }
  let resolved = topology;
  if (resolved == null) {
    try {
      resolved = consumeJourneyTopology(events, journey);
    } catch (error) {
      resolved = { ok: false, reasons: [error.message] };
    }
  }
  if (!resolved.ok) {
    const reason = `journey topology: ${resolved.reasons?.[0] || "mismatch"}`;
    return {
      skillDiscovered: cell("fail", reason),
      mcpStarted: cell("fail", reason),
      exactToolsListed: cell("fail", reason),
      oneToolInvoked: cell("fail", reason),
      structuredResultValidated: cell("unavailable", reason),
      missingBinaryNotClean: cell("fail", reason),
      invalidPolicyRootNotClean: cell("fail", reason),
      cwdObserved: cell("fail", reason),
      driverCompleted: classifyDriver(meta),
    };
  }
  const skills = skillsFromEvents(resolved.skills);
  const starts = resolved.threads;
  const statuses = resolved.statuses;
  const calls = mcpToolCalls(resolved.notifications ?? []);
  const threadId = primaryThreadId(starts);
  const turnId = turnIdFromTopology(resolved);
  const skillDiscovered = classifySkill(skills, expected);
  const cwdObserved = classifyCwd(starts, expected);
  const mcpStarted = classifyMcp(statuses);
  const exactToolsListed = classifyTools(statuses);
  const oneToolInvoked = classifyInvocation(calls, expected, threadId, turnId, resolved);
  const structuredResultValidated = classifyPayload(calls, oneToolInvoked);
  return {
    skillDiscovered,
    mcpStarted,
    exactToolsListed,
    oneToolInvoked,
    structuredResultValidated,
    missingBinaryNotClean: classifyNegative(statuses, 1, "missing-binary"),
    invalidPolicyRootNotClean: classifyNegative(
      statuses,
      2,
      "invalid-policy-root",
    ),
    cwdObserved,
    driverCompleted: classifyDriver(meta),
  };
}

export function liveAcceptance(cells, meta) {
  const reasons = [];
  if (meta.provenance !== "live") {
    reasons.push("synthetic fixtures never become live proof");
  }
  if (meta.userAgent === FAKE_USER_AGENT) {
    reasons.push("fake stdio child userAgent cannot be live");
  }
  if (!liveIdentityBound(meta.hostIdentity)) {
    reasons.push(
      "live record requires bound Codex and Assay/MCP binary paths, versions, hashes, install source, OS, and architecture",
    );
  } else if (
    meta.provenance === "live" &&
    !verifyLiveIdentity(meta.hostIdentity, meta.invocation, meta.topology)
  ) {
    reasons.push(
      "live identity must be the observed binaries bound to the actual thread/start and spawn commands",
    );
  }
  if (meta.truncated || meta.streamUnavailable) {
    reasons.push("host stream was truncated or unavailable");
  }
  if ((meta.driverOutcome?.exitCode ?? 1) !== 0) {
    reasons.push(
      `driver outcome exit ${meta.driverOutcome?.exitCode ?? 1} is preserved; not rewritten to 0`,
    );
  }
  for (const name of CELLS) {
    if (cells[name]?.status !== "pass") {
      reasons.push(`${name}=${cells[name]?.status}`);
    }
  }
  reasons.push(LIVE_IDENTITY_NONCLAIM);
  if (reasons.length > 1 || meta.provenance !== "live") {
    return cell("fail", reasons.join("; "));
  }
  return cell("pass", `live host-event evidence and driver completion; ${LIVE_IDENTITY_NONCLAIM}`);
}

export function classifyRecord(record) {
  const events = record.events;
  if (!Array.isArray(events)) {
    throw new Error("events must be an array");
  }
  const journey = record.journey ?? "tool";
  let topology;
  try {
    topology = consumeJourneyTopology(events, journey);
  } catch (error) {
    topology = { ok: false, reasons: [error.message], pairs: [] };
  }
  const derived = initializeFromTopology(topology);
  const meta = {
    provenance: record.provenance,
    childExitCode: record.childExitCode,
    driverOutcome: record.driverOutcome ?? null,
    truncated: Boolean(record.truncated),
    streamUnavailable: Boolean(record.streamUnavailable),
    userAgent: derived.userAgent,
    hostIdentity: record.hostIdentity ?? null,
    invocation: record.invocation ?? null,
    topology,
    events,
  };
  const cells = classifyCells(events, meta, record.expected, journey, topology);
  return {
    schema: SCHEMA,
    cells,
    liveAcceptance: liveAcceptance(cells, meta),
  };
}

function canonicalResolved(value) {
  const resolved = path.resolve(value);
  try {
    return fs.realpathSync(resolved);
  } catch {
    return resolved;
  }
}

function resolveExistingAncestor(value) {
  let current = path.resolve(value);
  const seen = new Set();
  while (!seen.has(current)) {
    seen.add(current);
    try {
      return fs.realpathSync(current);
    } catch {
      const parent = path.dirname(current);
      if (parent === current) {
        return current;
      }
      current = parent;
    }
  }
  return path.resolve(value);
}

function pathEqualsOrInside(root, candidate) {
  const rel = path.relative(root, candidate);
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
}

export function runtimeProofRoots(projectRoot, initialize) {
  const roots = [];
  if (typeof projectRoot === "string" && projectRoot.length > 0) {
    roots.push(path.join(projectRoot, ".codex-home"));
  }
  if (typeof initialize?.codexHome === "string" && initialize.codexHome.length > 0) {
    roots.push(initialize.codexHome);
  }
  return roots;
}

export function forbiddenProofRoot(proofRoot, provenance, extraRoots = []) {
  const resolved = resolveExistingAncestor(proofRoot);
  const temps = [os.tmpdir(), "/tmp", "/private/tmp", "/var/tmp"];
  const underTmp = temps.some((temp) => pathEqualsOrInside(canonicalResolved(temp), resolved));
  if (provenance === "live" && underTmp) {
    return "live proof root must not be temporary storage";
  }
  const runtimeRoots = [];
  if (process.env.CODEX_HOME) {
    runtimeRoots.push(process.env.CODEX_HOME);
  }
  if (process.env.HOME) {
    runtimeRoots.push(path.join(process.env.HOME, ".codex"));
  }
  if (Array.isArray(extraRoots)) {
    for (const root of extraRoots) {
      if (typeof root === "string" && root.length > 0) {
        runtimeRoots.push(root);
      }
    }
  }
  for (const root of runtimeRoots) {
    if (pathEqualsOrInside(canonicalResolved(root), resolved)) {
      return "proof root must not be equal to or inside CODEX_HOME, auth, or profile roots";
    }
  }
  if (resolved.endsWith(`${path.sep}auth.json`) || resolved.includes(`${path.sep}auth.json${path.sep}`)) {
    return "proof root must not be a credential path";
  }
  return null;
}

export function scrub(value) {
  if (typeof value === "string") {
    if (CREDENTIAL_KEY.test(value)) {
      return "[redacted]";
    }
    return value;
  }
  if (Array.isArray(value)) {
    return value.map(scrub);
  }
  if (value && typeof value === "object") {
    const out = {};
    for (const [key, child] of Object.entries(value)) {
      out[key] = CREDENTIAL_KEY.test(key) ? "[redacted]" : scrub(child);
    }
    return out;
  }
  return value;
}

const DEFAULT_MAX_BYTES = 1_048_576;

export function readBoundedFile(file, maxBytes) {
  const requested = boundedPositiveInt("maxBytes", maxBytes, HARD_MAX_BYTES);
  const limit = Math.min(requested, HARD_MAX_BYTES);
  const flags = fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0);
  const fd = fs.openSync(file, flags);
  try {
    const st = fs.fstatSync(fd);
    if (!st.isFile()) {
      throw new Error("input is not a regular file");
    }
    const buf = Buffer.alloc(limit + 1);
    const n = fs.readSync(fd, buf, 0, limit + 1, 0);
    if (n > limit) {
      throw new Error("input exceeds byte bound");
    }
    return buf.subarray(0, n);
  } finally {
    fs.closeSync(fd);
  }
}

export function parseArgs(argv) {
  const out = { proofRoot: null, maxBytes: DEFAULT_MAX_BYTES };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--proof-root") {
      i += 1;
      out.proofRoot = argv[i];
    } else if (argv[i] === "--max-bytes") {
      i += 1;
      out.maxBytes = boundedPositiveInt("maxBytes", Number(argv[i]), HARD_MAX_BYTES);
    } else {
      throw new Error(`unknown argument ${argv[i]}`);
    }
  }
  return out;
}

function namesIn(proofRoot) {
  const dir = fs.opendirSync(proofRoot);
  const names = [];
  try {
    let entry;
    while ((entry = dir.readSync()) !== null) {
      if (entry.name === "." || entry.name === "..") {
        continue;
      }
      names.push(entry.name);
      if (names.length > HARD_MAX_DIR_ENTRIES) {
        throw new Error("proof root directory listing exceeds bound");
      }
    }
  } finally {
    dir.closeSync();
  }
  return names;
}

function assertAllowlistedRegularFiles(proofRoot) {
  for (const name of ALLOWLIST) {
    const st = fs.lstatSync(path.join(proofRoot, name));
    if (st.isSymbolicLink()) {
      throw new Error(`${name} is a symlink`);
    }
    if (!st.isFile()) {
      throw new Error(`${name} is not a regular file`);
    }
  }
}

function canonicalManifestExpected(expected) {
  if (!expected || typeof expected !== "object") {
    return "manifest expected is missing";
  }
  if (
    !Array.isArray(expected.tools) ||
    !sameJson([...expected.tools].sort(), [...EXPECTED_TOOLS].sort())
  ) {
    return "manifest expected tools are not the canonical release set";
  }
  if (expected.toolName !== DECIDE_TOOL) {
    return "manifest expected toolName is not the canonical decide tool";
  }
  if (!sameJson(expected.toolArguments, DECIDE_INPUT)) {
    return "manifest expected toolArguments are not the canonical decide input";
  }
  if (expected.skillName !== SKILL_NAME) {
    return "manifest expected skillName is not canonical";
  }
  if (typeof expected.projectRoot !== "string" || expected.projectRoot.length === 0) {
    return "manifest expected projectRoot must be a run-specific path";
  }
  return null;
}

export function validateProofRoot(proofRoot, maxBytes = DEFAULT_MAX_BYTES) {
  boundedPositiveInt("maxBytes", maxBytes, HARD_MAX_BYTES);
  const reasons = [];
  const earlyForbidden = forbiddenProofRoot(proofRoot, "synthetic");
  if (earlyForbidden) {
    return { ok: false, reasons: [earlyForbidden], classified: null };
  }
  let manifest;
  let events;
  let stored;
  try {
    namesIn(proofRoot);
    assertAllowlistedRegularFiles(proofRoot);
    manifest = parseJsonBytes(
      readBoundedFile(path.join(proofRoot, "manifest.json"), maxBytes),
      maxBytes,
    );
    events = parseJsonBytes(
      readBoundedFile(path.join(proofRoot, "events.json"), maxBytes),
      maxBytes,
    );
    stored = parseJsonBytes(
      readBoundedFile(path.join(proofRoot, "classification.json"), maxBytes),
      maxBytes,
    );
  } catch (error) {
    return {
      ok: false,
      reasons: [`unavailable allowlisted proof: ${error.message}`],
      classified: null,
    };
  }
  if (manifest.schema !== SCHEMA) {
    reasons.push(`unexpected schema ${manifest.schema}`);
  }
  try {
    requiredCellsForJourney(manifest.journey);
  } catch (error) {
    reasons.push(error.message);
  }
  if (manifest.provenance === "live") {
    const liveForbidden = forbiddenProofRoot(proofRoot, "live");
    if (liveForbidden) {
      reasons.push(liveForbidden);
    }
  }
  const present = namesIn(proofRoot).sort();
  const allowed = [...ALLOWLIST].sort();
  if (stableStringify(present) !== stableStringify(allowed)) {
    reasons.push(
      `proof membership ${present.join(",")} is not the exact allowlist ${allowed.join(",")}`,
    );
  }
  const eventsText = stableStringify(events);
  const actual = sha256Utf8(eventsText);
  if (manifest.hashes?.events !== actual) {
    reasons.push("events hash mismatch");
  }
  const derivedInitialize = initializeFromEvents(events, manifest.journey ?? "tool");
  if (!sameJson(manifest.initialize, derivedInitialize)) {
    reasons.push("manifest initialize does not match captured initialize event");
  }
  const eventForbidden = forbiddenProofRoot(
    proofRoot,
    manifest.provenance === "live" ? "live" : "synthetic",
    runtimeProofRoots(manifest.expected?.projectRoot, derivedInitialize),
  );
  if (eventForbidden) {
    reasons.push(eventForbidden);
  }
  if (
    typeof manifest.childExitCode !== "number" ||
    !manifest.driverOutcome ||
    typeof manifest.driverOutcome.exitCode !== "number"
  ) {
    reasons.push("v2 record requires childExitCode and driverOutcome");
  }
  const expectedReason = canonicalManifestExpected(manifest.expected);
  if (expectedReason) {
    reasons.push(expectedReason);
  }
  if (!manifest.truncated && !manifest.streamUnavailable) {
    const topology = consumeJourneyTopology(events, manifest.journey ?? "tool");
    if (!topology.ok) {
      reasons.push(`journey topology: ${topology.reasons[0] || "mismatch"}`);
    }
  }
  const outcomeKind = closedDriverOutcomeStatus({
    childExitCode: manifest.childExitCode,
    driverOutcome: manifest.driverOutcome,
    truncated: Boolean(manifest.truncated),
    streamUnavailable: Boolean(manifest.streamUnavailable),
  });
  if (outcomeKind === "invalid" || outcomeKind === "preliminary") {
    reasons.push("childExitCode and driverOutcome violate the closed driver-outcome rule");
  }
  const record = {
    schema: manifest.schema,
    provenance: manifest.provenance,
    childExitCode: manifest.childExitCode,
    driverOutcome: manifest.driverOutcome,
    truncated: manifest.truncated,
    streamUnavailable: manifest.streamUnavailable,
    initialize: derivedInitialize,
    hostIdentity: manifest.hostIdentity ?? null,
    invocation: manifest.invocation ?? null,
    expected: manifest.expected,
    events,
    journey: manifest.journey,
  };
  let classified;
  try {
    classified = classifyRecord(record);
  } catch (error) {
    return { ok: false, reasons: [`classification failed: ${error.message}`], classified: null };
  }
  if (stableStringify(stored) !== stableStringify(classified)) {
    reasons.push("stored classification disagrees with recomputed classification");
  }
  if (manifest.provenance === "synthetic" && classified.liveAcceptance.status === "pass") {
    reasons.push("synthetic fixture claimed live pass");
  }
  if (manifest.provenance === "live" && manifest.initialize?.userAgent === FAKE_USER_AGENT) {
    reasons.push("live provenance with fake stdio child userAgent");
  }
  if (manifest.provenance === "live" && classified.liveAcceptance.status === "pass") {
    if (
      manifest.truncated ||
      manifest.streamUnavailable ||
      manifest.driverOutcome?.exitCode !== 0
    ) {
      reasons.push("live pass is incompatible with truncated, unavailable, or nonzero driver outcome");
    }
  }
  return {
    ok: reasons.length === 0,
    reasons,
    classified,
    manifest,
  };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (!options.proofRoot) {
    throw new Error("--proof-root is required");
  }
  const result = validateProofRoot(options.proofRoot, options.maxBytes);
  process.stdout.write(
    stableStringify({
      ok: result.ok,
      reasons: result.reasons,
      liveAcceptance: result.classified?.liveAcceptance ?? null,
      cells: result.classified?.cells ?? null,
    }),
  );
  if (!result.ok) {
    process.exitCode = 1;
  }
}

if (isMainModule(process.argv[1], import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
