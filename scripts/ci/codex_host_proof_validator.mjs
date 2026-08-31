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

export const SCHEMA = "assay.codex-host-proof.v1";
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
      calls.push(item);
    }
  }
  return calls;
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
      skill.path.startsWith(expected.projectRoot),
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

function classifyInvocation(calls, expected) {
  if (calls.length === 0) {
    return cell("unavailable", "no mcpToolCall item/completed");
  }
  if (calls.length !== 1) {
    return cell(
      "fail",
      `expected one mcpToolCall, found ${calls.length}`,
    );
  }
  const call = calls[0];
  if (call.tool !== expected.toolName) {
    return cell("fail", `wrong tool ${call.tool}`);
  }
  if (call.status !== "completed") {
    return cell("fail", `tool status ${call.status}`);
  }
  if (!sameJson(call.arguments, expected.toolArguments)) {
    return cell("fail", "tool arguments are not the pinned probe");
  }
  return cell("pass", `invoked ${expected.toolName}`);
}

function classifyPayload(calls, invocationCell) {
  if (invocationCell.status !== "pass") {
    return cell("unavailable", "invocation cell did not pass");
  }
  const payload = payloadFromCall(calls[0]);
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
  if (meta.driverExitCode === 0) {
    return cell("pass", "driver exited 0");
  }
  return cell("fail", `driver exited ${meta.driverExitCode}`);
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
  const skillDiscovered = classifySkill(skills, expected);
  const cwdObserved = classifyCwd(starts, expected);
  const mcpStarted = classifyMcp(statuses);
  const exactToolsListed = classifyTools(statuses);
  const oneToolInvoked = classifyInvocation(calls, expected);
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
  if (meta.truncated || meta.streamUnavailable) {
    reasons.push("host stream was truncated or unavailable");
  }
  if (meta.driverExitCode !== 0) {
    reasons.push(
      `original driver exit ${meta.driverExitCode} is preserved; not rewritten to 0`,
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
  const meta = {
    provenance: record.provenance,
    driverExitCode: record.driverExitCode,
    truncated: Boolean(record.truncated),
    streamUnavailable: Boolean(record.streamUnavailable),
    userAgent: record.initialize?.userAgent,
  };
  const cells = classifyCells(events, meta, record.expected);
  return {
    schema: SCHEMA,
    cells,
    liveAcceptance: liveAcceptance(cells, meta),
  };
}

export function forbiddenProofRoot(proofRoot, provenance) {
  const resolved = path.resolve(proofRoot);
  const temps = [os.tmpdir(), "/tmp", "/private/tmp", "/var/tmp"];
  const underTmp = temps.some(
    (temp) => resolved === path.resolve(temp) || resolved.startsWith(`${path.resolve(temp)}${path.sep}`),
  );
  if (provenance === "live" && underTmp) {
    return "live proof root must not be temporary storage";
  }
  const home = process.env.CODEX_HOME;
  if (home && resolved.startsWith(`${path.resolve(home)}${path.sep}`)) {
    return "proof root must not be inside CODEX_HOME runtime";
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

export function parseArgs(argv) {
  const out = { proofRoot: null, maxBytes: DEFAULT_MAX_BYTES };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--proof-root") {
      i += 1;
      out.proofRoot = argv[i];
    } else if (argv[i] === "--max-bytes") {
      i += 1;
      out.maxBytes = Number(argv[i]);
    } else {
      throw new Error(`unknown argument ${argv[i]}`);
    }
  }
  return out;
}

function namesIn(proofRoot) {
  return fs.readdirSync(proofRoot).filter((name) => name !== "." && name !== "..");
}

export function validateProofRoot(proofRoot, maxBytes = DEFAULT_MAX_BYTES) {
  const reasons = [];
  let manifest;
  let events;
  let stored;
  try {
    manifest = parseJsonBytes(
      fs.readFileSync(path.join(proofRoot, "manifest.json")),
      maxBytes,
    );
    events = parseJsonBytes(
      fs.readFileSync(path.join(proofRoot, "events.json")),
      maxBytes,
    );
    stored = parseJsonBytes(
      fs.readFileSync(path.join(proofRoot, "classification.json")),
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
  const record = {
    schema: manifest.schema,
    provenance: manifest.provenance,
    driverExitCode: manifest.driverExitCode,
    truncated: manifest.truncated,
    streamUnavailable: manifest.streamUnavailable,
    initialize: manifest.initialize,
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
    if (manifest.truncated || manifest.streamUnavailable || manifest.driverExitCode !== 0) {
      reasons.push("live pass is incompatible with truncated, unavailable, or nonzero driver exit");
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

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
