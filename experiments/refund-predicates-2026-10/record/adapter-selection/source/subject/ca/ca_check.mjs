// CA driver for the refund C3 adapter trial (CLAUDE-ADAPTER-DESIGN-v6 section 16). CA's entrypoint is
//   node ca/ca_check.mjs --config <config.json> {vector}
// It creates a private temp parent P under $C3_TRIAL_BASE and points TMPDIR at it, calls createAdapter(ctx).check
// directly, and prints exactly one JSON document {"outcome": <tuple or abnormal record>} plus LF. Exit 0 iff the
// outcome is a tuple, 3 for every abnormal record, 2 for a usage or setup error (no outcome is printed). P is removed
// when check resolves, unless its output reports process_cleanup "unconfirmed"; a throw or the soft deadline keeps P.
// The 10000 ms soft deadline is an observation only: it cannot fire during synchronous work, it terminates
// nothing, and on expiry the driver exits and abandons the facade. The enforced bound is CA's vector_timeout.
import { createHash } from 'node:crypto'
import * as fs from 'node:fs'
import * as path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { ADAPTER_VERSION, CLAIM_ID, parseStrictJson, serializeJson } from '../component/adapter.mjs'


export async function runDriver(integration) {
  if (!integration || typeof integration.beforeOutput !== 'function') throw new Error('trusted integration required');
const HERE = path.dirname(fileURLToPath(import.meta.url))
const OUTPUT_KEYS = ['adapter_version', 'admission_operation_id', 'claim', 'failure', 'front_request_sha256', 'inputs',
  'projector_record', 'projector_record_sha256', 'request_evidence_sha256', 'schema', 'selector', 'semantic']
const B64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/
const HEX64 = /^[0-9a-f]{64}$/
const LAYERS = ['facade', 'front', 'projector', 'native_output', 'postcondition', 'commitment']
const STATUSES = ['established', 'not_established', 'failed', 'unsupported']
const SEMANTIC_STATUSES = ['established', 'not_established']
const DETAIL_KEYS = ['front_exit', 'front_output_accepted', 'front_signal', 'front_stderr_bytes', 'front_stderr_error',
  'front_stderr_sha256', 'front_stderr_truncated', 'front_stdout_bytes', 'front_stdout_error', 'front_stdout_sha256',
  'front_stdout_truncated', 'process_cleanup', 'workdir_cleanup']
const COMMIT_NAMES = ['case_id', 'packet_sha256', 'native_output_sha256', 'evidence_operation_id', 'report_id', 'report_seq',
  'axis', 'native_applicable', 'native_result', 'native_reason', 'context_delivery_coverage', 'context_source_class',
  'context_conflict', 'projected', 'selection_rule', 'mapping_revision']

function usage(message) {
  throw new Error(`ca_check usage: ${String(message).slice(0, 200)}`)
}

function parseArgs(argv) {
  const opts = { config: null, adapter: path.join(HERE, '..', 'component', 'adapter.mjs'), softMs: 10000, vector: null }
  for (let k = 0; k < argv.length; k++) {
    const a = argv[k]
    if (a === '--config') opts.config = argv[++k]
    else if (a === '--adapter') opts.adapter = argv[++k]
    else if (a === '--soft-ms') opts.softMs = Number(argv[++k])
    else if (opts.vector === null) opts.vector = a
    else usage('unexpected argument')
  }
  if (!opts.config || !opts.vector || !Number.isSafeInteger(opts.softMs) || opts.softMs <= 0) usage('need --config and a vector')
  return opts
}

function emit(outcome, code) {
  integration.beforeOutput()
  process.stdout.write(serializeJson({ outcome }) + '\n', () => process.exit(code))
}

const abnormal = (kind, message) => emit({ abnormal: kind, message: String(message).slice(0, 200) }, 3)

const isObject = v => typeof v === 'object' && v !== null && !Array.isArray(v)

function sameKeys(obj, keys) {
  const own = Object.keys(obj).sort()
  const want = [...keys].sort()
  return own.length === want.length && own.every((k, i) => k === want[i])
}

const isHex = v => typeof v === 'string' && HEX64.test(v)
const intOrNull = v => v === null || Number.isSafeInteger(v)
const strOrNull = v => v === null || typeof v === 'string'
const hexOrNull = v => v === null || isHex(v)
const boolOrNull = v => v === null || typeof v === 'boolean'

function validDetail(d) {
  return isObject(d) && sameKeys(d, DETAIL_KEYS) && intOrNull(d.front_exit) && strOrNull(d.front_signal) &&
    hexOrNull(d.front_stdout_sha256) && intOrNull(d.front_stdout_bytes) && boolOrNull(d.front_stdout_truncated) &&
    hexOrNull(d.front_stderr_sha256) && intOrNull(d.front_stderr_bytes) && boolOrNull(d.front_stderr_truncated) &&
    strOrNull(d.front_stdout_error) && strOrNull(d.front_stderr_error) && typeof d.front_output_accepted === 'boolean' &&
    ['complete', 'killed', 'unconfirmed', 'not_started'].includes(d.process_cleanup) &&
    ['removed', 'remove_failed', 'not_created'].includes(d.workdir_cleanup)
}

// Shape only, plus the coherence the tuple relies on (projected equals the claim status). It never recomputes a
// verdict from native or context: those are only required to be objects, so a semantic movement stays observable.
function validSemantic(s, status) {
  if (!isObject(s) || !sameKeys(s, ['context', 'native', 'projected', 'selected', 'selected_report_commitment'])) return false
  const sel = s.selected
  if (!isObject(sel) || !sameKeys(sel, ['case_id', 'evidence_operation_id', 'report_id', 'report_seq'])) return false
  if (typeof sel.case_id !== 'string' || typeof sel.evidence_operation_id !== 'string' || typeof sel.report_id !== 'string') return false
  if (!Number.isSafeInteger(sel.report_seq) || !isObject(s.native) || !isObject(s.context) || s.projected !== status) return false
  const k = s.selected_report_commitment
  if (!isObject(k) || !sameKeys(k, ['fields', 'profile', 'sha256']) || k.profile !== 'refund.c3-selected-report.v0' || !isHex(k.sha256)) return false
  return Array.isArray(k.fields) && k.fields.length === COMMIT_NAMES.length &&
    k.fields.every((f, i) => Array.isArray(f) && f.length === 2 && f[0] === COMMIT_NAMES[i] && typeof f[1] === 'string')
}

// Design v6 section 8 for the evidence of this input: closed keys, exact schema and version, the admission id and
// evidence digest of this check, then the per-origin null rules. Facade origin is a failure with a detail object.
function validOutput(doc, operationId, evidenceSha) {
  if (!isObject(doc) || !sameKeys(doc, OUTPUT_KEYS)) return false
  if (doc.schema !== 'refund.c3-adapter-output.v0' || doc.adapter_version !== ADAPTER_VERSION) return false
  if (doc.admission_operation_id !== operationId || doc.request_evidence_sha256 !== evidenceSha) return false
  const c = doc.claim
  if (!isObject(c) || !sameKeys(c, ['claim', 'reason', 'status']) || c.claim !== CLAIM_ID) return false
  if (!STATUSES.includes(c.status) || typeof c.reason !== 'string') return false
  const semanticStatus = SEMANTIC_STATUSES.includes(c.status)
  const f = doc.failure
  if (f === null) {
    if (!semanticStatus) return false
  } else if (semanticStatus || !isObject(f) || !sameKeys(f, ['code', 'detail', 'layer']) || !LAYERS.includes(f.layer) || f.code !== c.reason) {
    return false
  }
  if (f !== null && f.detail !== null) {
    if (c.status !== 'failed' || !hexOrNull(doc.front_request_sha256)) return false
    for (const k of ['inputs', 'selector', 'projector_record', 'projector_record_sha256', 'semantic']) if (doc[k] !== null) return false
    return validDetail(f.detail)
  }
  if (!isHex(doc.front_request_sha256)) return false
  for (const k of ['inputs', 'selector', 'projector_record']) if (doc[k] !== null && !isObject(doc[k])) return false
  if ((doc.projector_record === null) !== (doc.projector_record_sha256 === null) || !hexOrNull(doc.projector_record_sha256)) return false
  // Design v6 section 8 (lines 571-573): a semantic success has passed P1e and P5b and carries the claim record, so its
  // inputs, selector and projector record are present. A failure keeps its stage-dependent nulls.
  if (semanticStatus && (doc.inputs === null || doc.selector === null || doc.projector_record === null)) return false
  return semanticStatus ? validSemantic(doc.semantic, c.status) : doc.semantic === null
}

const opts = parseArgs(process.argv.slice(2))
const base = process.env.C3_TRIAL_BASE
if (!base || !path.isAbsolute(base) || !fs.existsSync(base)) usage('C3_TRIAL_BASE must name an existing absolute directory')

let vector, config
try {
  vector = parseStrictJson(fs.readFileSync(opts.vector, 'utf8'))
  config = parseStrictJson(fs.readFileSync(opts.config, 'utf8'))
} catch (e) {
  usage(`unreadable vector or config: ${e.message}`)
}
if (!isObject(vector) || !sameKeys(vector, ['evidence_b64', 'id', 'operation_id', 'schema']) ||
    vector.schema !== 'refund.c3-trial-vector.v0' || typeof vector.operation_id !== 'string' ||
    typeof vector.evidence_b64 !== 'string' || !B64.test(vector.evidence_b64)) usage('malformed vector')

let P
try {
  P = integration.allocateParent(path.join(base, 'v-'))
} catch (e) {
  usage(`cannot create the private temp parent: ${e.code || e.message}`)
}
process.env.TMPDIR = P

let mod
try {
  mod = await import(pathToFileURL(path.resolve(opts.adapter)).href)
} catch (e) {
  usage(`cannot import the adapter: ${e.code || e.message}`)
}
const evidence = new Uint8Array(Buffer.from(vector.evidence_b64, 'base64'))
const evidenceSha = createHash('sha256').update(evidence).digest('hex')
const input = {
  operation_id: vector.operation_id,
  workflow: 'refund.c3-trial',
  action: { tool: 'none', args: {} },
  evidence,
  now: new Date().toISOString(),
}

let timer
const soft = new Promise(resolve => { timer = setTimeout(() => resolve({ timedOut: true }), opts.softMs) })
let result
try {
  const adapter = integration.createAdapter(mod, { config, secrets: {}, fetch: undefined, integration })
  result = await Promise.race([adapter.check(input).then(value => ({ value })), soft])
} catch (e) {
  // A throw is not a normal return and proves nothing about descendants, so P is kept for the trial-level cleanup.
  abnormal('check_threw', e && e.message)
}
if (result) {
  if (result.timedOut) {
    abnormal('driver_timeout', `check did not resolve within the ${opts.softMs} ms soft deadline`)
  } else {
    clearTimeout(timer)
    finish(result.value)
  }
}

// The outcome record of a check that resolved: {line} for a tuple, or {abnormal, message}. liveChild is set when a
// section 8 output's facade-origin detail reports process_cleanup "unconfirmed".
function project(out) {
  if (!isObject(out) || !(out.evidence instanceof Uint8Array) || !Array.isArray(out.claims) || out.claims.length !== 1) {
    return { abnormal: 'output_shape', message: 'CheckOutput shape' }
  }
  let doc
  try {
    doc = parseStrictJson(new TextDecoder('utf-8', { fatal: true }).decode(out.evidence))
  } catch (e) {
    return { abnormal: 'evidence_unparseable', message: e.message }
  }
  if (!validOutput(doc, input.operation_id, evidenceSha)) {
    return { abnormal: 'evidence_unparseable', message: 'not a refund.c3-adapter-output.v0 document for this input' }
  }
  const liveChild = doc.failure !== null && doc.failure.detail !== null && doc.failure.detail.process_cleanup === 'unconfirmed'
  const c = out.claims[0]
  // Design v6 section 16: claims[0] equals evidence.claim, the closed {claim, status, reason}; key order is irrelevant.
  if (!isObject(c) || !sameKeys(c, ['claim', 'reason', 'status']) || c.claim !== doc.claim.claim ||
      c.status !== doc.claim.status || c.reason !== doc.claim.reason) {
    return { abnormal: 'claim_evidence_mismatch', message: 'claims[0] differs from evidence.claim', liveChild }
  }
  try {
    const failure = doc.failure
    const semantic = doc.semantic
    const tuple = [
      doc.claim.status,
      doc.claim.reason,
      failure === null ? null : failure.layer,
      failure === null ? null : failure.code,
      semantic === null ? null : semantic.selected,
      semantic === null ? null : semantic.selected_report_commitment.sha256,
      doc.projector_record_sha256,
    ]
    return { line: serializeJson({ outcome: tuple }), liveChild }
  } catch (e) {
    return { abnormal: 'evidence_unparseable', message: `tuple projection failed: ${e.message}`, liveChild }
  }
}

// Design v6 section 16.1 removes P on a normal return. A check that resolved is a normal return whatever outcome
// record it yields, so P is removed before that record is written. The exception is liveChild: the facade itself
// reports that a descendant may still be alive, so P is kept for the trial-level cleanup after the self-deadline
// horizon, as on the soft deadline. A removal that fails also leaves P for that cleanup; nothing here reports P as
// removed.
function finish(out) {
  integration.beforeOutput()
  const r = project(out)
  if (!r.liveChild) {
    try { fs.rmSync(P, { recursive: true, force: true }) } catch {}
  }
  if (r.line === undefined) return abnormal(r.abnormal, r.message)
  return process.stdout.write(r.line + '\n', () => process.exit(0))
}

}
