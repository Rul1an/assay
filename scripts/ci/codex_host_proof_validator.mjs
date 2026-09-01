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

export function initializeFromEvents(events) {
  const event = Array.isArray(events)
    ? events.find((row) => row.method === "initialize" && row.direction === "server")
    : null;
  const result = event?.result && typeof event.result === "object" ? event.result : {};
  return {
    codexHome: result.codexHome ?? null,
    userAgent: result.userAgent ?? null,
    platformFamily: result.platformFamily ?? null,
    platformOs: result.platformOs ?? null,
  };
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

export function driverOutcomeExit(pack, cells, journey) {
  const child = pack.childExit;
  const fail = child && child !== 0 ? child : 1;
  if (pack.truncated || pack.streamUnavailable) {
    return fail;
  }
  if (cells?.driverCompleted?.status === "unavailable") {
    return fail;
  }
  if (journey === "tool" || journey === "failures") {
    for (const name of CELLS) {
      if (name === "driverCompleted") {
        continue;
      }
      if (cells?.[name]?.status !== "pass") {
        return fail;
      }
    }
  }
  if (journey === "discovery" && cells?.skillDiscovered?.status === "unavailable") {
    return fail;
  }
  if (child !== 0) {
    return fail;
  }
  return 0;
}

export function driverOutcomeFrom(pack, cells, journey) {
  const exitCode = driverOutcomeExit(pack, cells, journey);
  let status = "pass";
  if (pack.truncated || pack.streamUnavailable) {
    status = "unavailable";
  } else if (exitCode !== 0) {
    status = "fail";
  }
  return { exitCode, status };
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

function threadStarts(events) {
  return events.filter(
    (event) => event.method === "thread/start" && event.direction === "server",
  );
}

function mcpStatuses(events) {
  return events.filter(
    (event) =>
      event.method === "mcpServerStatus/list" && event.direction === "server",
  );
}

function mcpToolCalls(events) {
  const calls = [];
  for (const event of events) {
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

function actualTurnId(events) {
  const reply = events.find(
    (event) => event.method === "turn/start" && event.direction === "server",
  );
  const id = reply?.result?.turn?.id;
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
  const enabled = found.find(
    (skill) =>
      skill.enabled === true &&
      typeof skill.path === "string" &&
      pathInsideRoot(expected.projectRoot, skill.path),
  );
  if (!enabled) {
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

function classifyMcp(statuses) {
  const primary = statuses[0]?.result?.data;
  const assay = Array.isArray(primary)
    ? primary.find((row) => row?.name === "assay")
    : null;
  if (!assay) {
    return cell("unavailable", "no assay mcpServerStatus row");
  }
  if (assay.runtimeStatus !== "connected") {
    return cell("fail", `runtimeStatus ${assay.runtimeStatus}`);
  }
  return cell("pass", "assay MCP connected");
}

function classifyTools(statuses) {
  const primary = statuses[0]?.result?.data;
  const assay = Array.isArray(primary)
    ? primary.find((row) => row?.name === "assay")
    : null;
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

function classifyInvocation(calls, expected, threadId, turnId) {
  if (calls.length === 0) {
    return cell("unavailable", "no mcpToolCall item/completed");
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
  if (item.tool !== expected.toolName) {
    return cell("fail", `wrong tool ${item.tool}`);
  }
  if (item.status !== "completed") {
    return cell("fail", `tool status ${item.status}`);
  }
  if (!sameJson(item.arguments, expected.toolArguments)) {
    return cell("fail", "tool arguments are not the pinned probe");
  }
  return cell("pass", `invoked ${expected.toolName}`);
}

function classifyPayload(calls, invocationCell) {
  if (invocationCell.status !== "pass") {
    return cell("unavailable", "invocation cell did not pass");
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
  const row = statuses[index]?.result?.data;
  const assay = Array.isArray(row)
    ? row.find((entry) => entry?.name === "assay")
    : null;
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
  if (meta.streamUnavailable) {
    return cell("unavailable", "stdio stream unavailable");
  }
  if (meta.truncated) {
    return cell("unavailable", "stdio truncated at bound");
  }
  const outcome = meta.driverOutcome;
  if (outcome && typeof outcome === "object") {
    if (outcome.status === "unavailable") {
      return cell("unavailable", "driver outcome unavailable");
    }
    if (outcome.exitCode === 0) {
      return cell("pass", "driver outcome exit 0");
    }
    return cell("fail", `driver outcome exit ${outcome.exitCode}`);
  }
  if (meta.childExitCode === 0) {
    return cell("pass", "child exited 0 pending driver outcome");
  }
  return cell("fail", `child exited ${meta.childExitCode}`);
}

export function classifyCells(events, meta, expected) {
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
  const skills = skillsFromEvents(events);
  const starts = threadStarts(events);
  const statuses = mcpStatuses(events);
  const calls = mcpToolCalls(events);
  const threadId = primaryThreadId(starts);
  const turnId = actualTurnId(events);
  const skillDiscovered = classifySkill(skills, expected);
  const cwdObserved = classifyCwd(starts, expected);
  const mcpStarted = classifyMcp(statuses);
  const exactToolsListed = classifyTools(statuses);
  const oneToolInvoked = classifyInvocation(calls, expected, threadId, turnId);
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
  if (reasons.length > 0) {
    return cell("fail", reasons.join("; "));
  }
  return cell("pass", "live host-event evidence and driver completion");
}

export function classifyRecord(record) {
  const events = record.events;
  if (!Array.isArray(events)) {
    throw new Error("events must be an array");
  }
  const derived = initializeFromEvents(events);
  const meta = {
    provenance: record.provenance,
    childExitCode: record.childExitCode,
    driverOutcome: record.driverOutcome ?? null,
    truncated: Boolean(record.truncated),
    streamUnavailable: Boolean(record.streamUnavailable),
    userAgent: derived.userAgent,
    hostIdentity: record.hostIdentity ?? null,
  };
  const cells = classifyCells(events, meta, record.expected);
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

function pathEqualsOrInside(root, candidate) {
  const rel = path.relative(root, candidate);
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
}

export function forbiddenProofRoot(proofRoot, provenance) {
  const resolved = canonicalResolved(proofRoot);
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
  const names = fs.readdirSync(proofRoot).filter((name) => name !== "." && name !== "..");
  if (names.length > HARD_MAX_DIR_ENTRIES) {
    throw new Error("proof root directory listing exceeds bound");
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

export function validateProofRoot(proofRoot, maxBytes = DEFAULT_MAX_BYTES) {
  boundedPositiveInt("maxBytes", maxBytes, HARD_MAX_BYTES);
  const reasons = [];
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
  const derivedInitialize = initializeFromEvents(events);
  if (!sameJson(manifest.initialize, derivedInitialize)) {
    reasons.push("manifest initialize does not match captured initialize event");
  }
  if (
    typeof manifest.childExitCode !== "number" ||
    !manifest.driverOutcome ||
    typeof manifest.driverOutcome.exitCode !== "number"
  ) {
    reasons.push("v2 record requires childExitCode and driverOutcome");
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
    expected: manifest.expected,
    events,
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
