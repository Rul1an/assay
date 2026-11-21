// federation-port/v0 facade for the refund C3 retained-report claim.
// Contract: CLAUDE-ADAPTER-DESIGN-v6 (sha256 c7f7c18b...) sections 2-11, with the v6 addenda. The facade validates
// config and component bytes, writes the request into a private work dir T, runs front.py in its own process
// group, settles the group on bounded deadlines, removes T, and decides the result in the step order of v6 section 6.
import * as childProcess from 'node:child_process'
import { createHash } from 'node:crypto'
import * as fs from 'node:fs'
import * as os from 'node:os'
import * as path from 'node:path'
import { fileURLToPath } from 'node:url'

export const ADAPTER_VERSION = '0.1.0'
export const CLAIM_ID = 'c3.retained_report.definite_support.projected'
const OUTPUT_SCHEMA = 'refund.c3-adapter-output.v0'
const CONFIG_SCHEMA = 'refund.c3-adapter-config.v0'
const REQUEST_SCHEMA = 'refund.c3-front-request.v0'
const D_RUN_MS = 6000
const D_SETTLE_MS = 1000
const PROBE_MS = 100
const STDOUT_BOUND = 1048576
const STDERR_BOUND = 65536
const MAX_DEPTH = 32
const HEX64 = /^[0-9a-f]{64}$/
const HEX40 = /^[0-9a-f]{40}$/
const ERROR_CODE = /^[A-Z0-9_]{1,64}$/
const LAYERS = ['facade', 'front', 'projector', 'native_output', 'postcondition', 'commitment']
const STATUSES = ['established', 'not_established', 'failed', 'unsupported']
const OUTPUT_KEYS = ['adapter_version', 'admission_operation_id', 'claim', 'failure', 'front_request_sha256', 'inputs',
  'projector_record', 'projector_record_sha256', 'request_evidence_sha256', 'schema', 'selector', 'semantic']

const componentDir = path.dirname(fileURLToPath(import.meta.url))
const sha256hex = bytes => createHash('sha256').update(bytes).digest('hex')
const now = () => performance.now()

// ---------------------------------------------------------------------------------------------------------------
// Serialization (v6 section 8): sorted keys, no whitespace, integers only, CPython ensure_ascii string escapes.

function escapeString(s) {
  return JSON.stringify(s).replace(/[\u007f-￿]/g, c => '\\u' + c.charCodeAt(0).toString(16).padStart(4, '0'))
}

export function serializeJson(value) {
  if (value === null) return 'null'
  if (value === true) return 'true'
  if (value === false) return 'false'
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value)) throw new TypeError('serialize_non_integer')
    return String(value)
  }
  if (typeof value === 'string') return escapeString(value)
  if (Array.isArray(value)) return '[' + value.map(serializeJson).join(',') + ']'
  if (isPlainObject(value)) {
    return '{' + Object.keys(value).sort().map(k => escapeString(k) + ':' + serializeJson(value[k])).join(',') + '}'
  }
  throw new TypeError('serialize_unsupported')
}

// ---------------------------------------------------------------------------------------------------------------
// Strict JSON (v6 section 8, step 11): RFC 8259 grammar; duplicate keys, depth over 32 and trailing data refused.

const NUMBER = /-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/y
const SIMPLE_ESCAPES = { '"': '"', '\\': '\\', '/': '/', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' }

export function parseStrictJson(text) {
  let i = 0
  const n = text.length
  const fail = what => { throw new SyntaxError(`strict_json:${what}@${i}`) }
  const ws = () => {
    while (i < n) {
      const c = text.charCodeAt(i)
      if (c === 0x20 || c === 0x09 || c === 0x0a || c === 0x0d) i++
      else break
    }
  }
  const literal = (word, v) => {
    if (!text.startsWith(word, i)) fail('literal')
    i += word.length
    return v
  }
  const number = () => {
    NUMBER.lastIndex = i
    const m = NUMBER.exec(text)
    if (!m) fail('number')
    i += m[0].length
    const v = Number(m[0])
    if (!Number.isFinite(v)) fail('number_range')
    return v
  }
  const string = () => {
    i++
    let out = ''
    let start = i
    for (;;) {
      if (i >= n) fail('unterminated_string')
      const c = text.charCodeAt(i)
      if (c === 0x22) { out += text.slice(start, i); i++; return out }
      if (c < 0x20) fail('control_in_string')
      if (c === 0x5c) {
        out += text.slice(start, i)
        const e = text[i + 1]
        if (e === 'u') {
          const hex = text.slice(i + 2, i + 6)
          if (!/^[0-9a-fA-F]{4}$/.test(hex)) fail('unicode_escape')
          out += String.fromCharCode(parseInt(hex, 16))
          i += 6
        } else {
          if (e === undefined || !Object.hasOwn(SIMPLE_ESCAPES, e)) fail('escape')
          out += SIMPLE_ESCAPES[e]
          i += 2
        }
        start = i
        continue
      }
      i++
    }
  }
  const value = depth => {
    ws()
    if (i >= n) fail('eof')
    const c = text[i]
    if (c === '{') return object(depth + 1)
    if (c === '[') return array(depth + 1)
    if (c === '"') return string()
    if (c === 't') return literal('true', true)
    if (c === 'f') return literal('false', false)
    if (c === 'n') return literal('null', null)
    if (c === '-' || (c >= '0' && c <= '9')) return number()
    return fail('unexpected')
  }
  const object = depth => {
    if (depth > MAX_DEPTH) fail('depth')
    i++
    const obj = {}
    const seen = new Set()
    ws()
    if (text[i] === '}') { i++; return obj }
    for (;;) {
      ws()
      if (text[i] !== '"') fail('key')
      const key = string()
      if (seen.has(key)) fail('duplicate_key')
      seen.add(key)
      ws()
      if (text[i] !== ':') fail('colon')
      i++
      Object.defineProperty(obj, key, { value: value(depth), enumerable: true, writable: true, configurable: true })
      ws()
      if (text[i] === ',') { i++; continue }
      if (text[i] === '}') { i++; return obj }
      fail('object')
    }
  }
  const array = depth => {
    if (depth > MAX_DEPTH) fail('depth')
    i++
    const arr = []
    ws()
    if (text[i] === ']') { i++; return arr }
    for (;;) {
      arr.push(value(depth))
      ws()
      if (text[i] === ',') { i++; continue }
      if (text[i] === ']') { i++; return arr }
      fail('array')
    }
  }
  const v = value(0)
  ws()
  if (i !== n) fail('trailing')
  return v
}

export function parseFrontLine(buf) {
  const len = buf.length
  if (len < 2 || buf[len - 1] !== 0x0a) throw new SyntaxError('framing:lf')
  if (buf[0] === 0xef && buf[1] === 0xbb && buf[2] === 0xbf) throw new SyntaxError('framing:bom')
  for (let k = 0; k < len - 1; k++) {
    if (buf[k] === 0x0a || buf[k] === 0x0d) throw new SyntaxError('framing:line')
  }
  const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(buf.subarray(0, len - 1))
  return parseStrictJson(text)
}

// ---------------------------------------------------------------------------------------------------------------
// Shapes.

function isPlainObject(v) {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return false
  const proto = Object.getPrototypeOf(v)
  return proto === Object.prototype || proto === null
}

function exactKeys(obj, keys) {
  const own = Object.keys(obj).sort()
  const want = [...keys].sort()
  return own.length === want.length && own.every((k, idx) => k === want[idx])
}

const isHex64 = v => typeof v === 'string' && HEX64.test(v)

// Step 11 shape check of a front-origin output (v6 section 8). It does not re-validate projector_record or
// semantic content; Q and K own those.
export function validateFrontOutput(doc, expected) {
  if (!isPlainObject(doc) || !exactKeys(doc, OUTPUT_KEYS)) return false
  if (doc.schema !== OUTPUT_SCHEMA || doc.adapter_version !== ADAPTER_VERSION) return false
  if (doc.admission_operation_id !== expected.operationId) return false
  if (doc.request_evidence_sha256 !== expected.evidenceSha || doc.front_request_sha256 !== expected.requestSha) return false
  const claim = doc.claim
  if (!isPlainObject(claim) || !exactKeys(claim, ['claim', 'reason', 'status'])) return false
  if (claim.claim !== CLAIM_ID || !STATUSES.includes(claim.status) || typeof claim.reason !== 'string') return false
  const semanticStatus = claim.status === 'established' || claim.status === 'not_established'
  if (doc.failure === null) {
    if (!semanticStatus) return false
  } else {
    const f = doc.failure
    if (semanticStatus || !isPlainObject(f) || !exactKeys(f, ['code', 'detail', 'layer'])) return false
    if (f.detail !== null || !LAYERS.includes(f.layer) || f.code !== claim.reason) return false
  }
  if (semanticStatus ? !isPlainObject(doc.semantic) : doc.semantic !== null) return false
  for (const k of ['inputs', 'selector', 'projector_record']) {
    if (doc[k] !== null && !isPlainObject(doc[k])) return false
  }
  if ((doc.projector_record === null) !== (doc.projector_record_sha256 === null)) return false
  if (doc.projector_record_sha256 !== null && !isHex64(doc.projector_record_sha256)) return false
  return true
}

// Step 2 (v6 section 3): the closed config, at every level.
function validConfig(c) {
  if (!isPlainObject(c) || !exactKeys(c, ['authorized_component_tuples', 'native_output_allowlist', 'python', 'schema'])) return false
  if (c.schema !== CONFIG_SCHEMA) return false
  const p = c.python
  if (!isPlainObject(p) || !exactKeys(p, ['path', 'version_observation'])) return false
  if (typeof p.path !== 'string' || p.path.length < 1 || p.path.length > 4096 || p.path[0] !== '/' || p.path.includes('\0')) return false
  if (typeof p.version_observation !== 'string') return false
  const tuples = c.authorized_component_tuples
  if (!Array.isArray(tuples) || tuples.length === 0) return false
  for (const t of tuples) {
    if (!isPlainObject(t) || !exactKeys(t, ['front_py', 'label', 'projector_py', 'replay_native_py'])) return false
    if (typeof t.label !== 'string' || !isHex64(t.front_py) || !isHex64(t.projector_py) || !isHex64(t.replay_native_py)) return false
  }
  const allow = c.native_output_allowlist
  if (!Array.isArray(allow) || allow.length === 0) return false
  const seen = new Set()
  for (const a of allow) {
    if (!isPlainObject(a) || !exactKeys(a, ['label', 'provenance', 'sha256', 'source'])) return false
    if (!isHex64(a.sha256) || seen.has(a.sha256) || typeof a.label !== 'string') return false
    seen.add(a.sha256)
    if (a.source === 'retained-observation') {
      const pv = a.provenance
      if (!isPlainObject(pv) || !exactKeys(pv, ['blob_path', 'lab_commit', 'receipt_path'])) return false
      if (typeof pv.lab_commit !== 'string' || !HEX40.test(pv.lab_commit)) return false
      if (typeof pv.blob_path !== 'string' || typeof pv.receipt_path !== 'string') return false
    } else if (a.source === 'controlled-seam') {
      if (a.provenance !== null) return false
    } else return false
  }
  return true
}

// Step 3 (v6 section 10): the hashed triple must equal one authorized tuple.
function tupleAuthorized(config) {
  let hashes
  try {
    hashes = ['front.py', 'replay_native.py', 'projector.py'].map(f => sha256hex(fs.readFileSync(path.join(componentDir, 'py', f))))
  } catch {
    return false
  }
  const [front, replay, projector] = hashes
  return config.authorized_component_tuples.some(t => t.front_py === front && t.replay_native_py === replay && t.projector_py === projector)
}

function removeDir(T) {
  try {
    fs.rmSync(T, { recursive: true, force: true })
    return 'removed'
  } catch {
    return 'remove_failed'
  }
}

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const errorCode = e => (e && typeof e.code === 'string' && ERROR_CODE.test(e.code) ? e.code : 'unknown')

// ---------------------------------------------------------------------------------------------------------------
// The check.

function facadeOutput(st, layer, code) {
  const doc = {
    schema: OUTPUT_SCHEMA,
    adapter_version: ADAPTER_VERSION,
    admission_operation_id: st.operationId,
    request_evidence_sha256: st.evidenceSha,
    front_request_sha256: st.requestSha,
    inputs: null,
    selector: null,
    projector_record: null,
    projector_record_sha256: null,
    semantic: null,
    failure: { layer, code, detail: st.detail() },
    claim: { claim: CLAIM_ID, status: 'failed', reason: code },
  }
  return { evidence: new Uint8Array(Buffer.from(serializeJson(doc), 'utf8')), claims: [{ claim: CLAIM_ID, status: 'failed', reason: code }] }
}

async function runCheck(config, input, deps) {
  // Trusted injected seams for finite pure controls only; createAdapter uses real defaults.
  const fileIO = deps.fileIO || fs
  const componentAuthorized = deps.componentAuthorized || tupleAuthorized
  const removeWorkdir = deps.removeWorkdir || removeDir
  const clock = deps.now || now
  const delay = deps.sleep || sleep
  const setTimer = deps.setTimer || setTimeout
  const clearTimer = deps.clearTimer || clearTimeout
  let operationalFirst = null
  const operationalSecondary = []
  const operational = (reason, phase, error=null) => {
    const cls = error ? String(error.constructor?.name || 'Error').slice(0,64) : null
    const fact = {reason,phase,class:cls,code:error&&typeof error.code==='string'&&/^[A-Za-z0-9_:-]{1,64}$/.test(error.code)?error.code:null,
      errno:error&&Number.isSafeInteger(error.errno)&&Math.abs(error.errno)<=4096?error.errno:null}
    if(operationalFirst===null) operationalFirst=fact
    else if(operationalSecondary.length<4) operationalSecondary.push(fact)
  }
  const evidence = input && input.evidence instanceof Uint8Array ? input.evidence : undefined
  const run = {
    spawned: false, exit: null, closed: false, accepted: false,
    stdout: [], stdoutBytes: 0, stdoutTruncated: false, stdoutError: null,
    stderr: [], stderrBytes: 0, stderrTruncated: false, stderrError: null,
    timeoutAt: null, boundAt: null, destroyedByFacade: false,
  }
  const st = {
    operationId: input && typeof input.operation_id === 'string' ? input.operation_id : '',
    evidenceSha: evidence ? sha256hex(evidence) : null,
    requestSha: null,
    processCleanup: 'not_started',
    workdirCleanup: 'not_created',
    detail() {
      const stream = name => (run.spawned
        ? { sha: sha256hex(Buffer.concat(run[name])), bytes: run[name + 'Bytes'], truncated: run[name + 'Truncated'] }
        : { sha: null, bytes: null, truncated: null })
      const out = stream('stdout')
      const err = stream('stderr')
      return {
        front_exit: run.exit && run.exit.signal === null ? run.exit.code : null,
        front_signal: run.exit && run.exit.signal !== null ? run.exit.signal : null,
        front_stdout_sha256: out.sha, front_stdout_bytes: out.bytes, front_stdout_truncated: out.truncated,
        front_stderr_sha256: err.sha, front_stderr_bytes: err.bytes, front_stderr_truncated: err.truncated,
        front_stdout_error: run.stdoutError, front_stderr_error: run.stderrError,
        front_output_accepted: run.accepted,
        process_cleanup: st.processCleanup,
        workdir_cleanup: st.workdirCleanup,
      }
    },
  }

  const observed = () => ({first_failure:operationalFirst,secondary:[...operationalSecondary],front:{
    exit:run.exit,close_seen:run.closed,stdout_sha256:sha256hex(Buffer.concat(run.stdout)),stdout_bytes:run.stdoutBytes,
    stdout_truncated:run.stdoutTruncated,stdout_error:run.stdoutError,stderr:Buffer.concat(run.stderr),
    stderr_sha256:sha256hex(Buffer.concat(run.stderr)),stderr_bytes:run.stderrBytes,stderr_truncated:run.stderrTruncated,
    stderr_error:run.stderrError,destroyed_by_facade:run.destroyedByFacade}})

  // Steps 1-4.
  if (process.platform !== 'linux' && process.platform !== 'darwin') return facadeOutput(st, 'facade', 'adapter:platform_unsupported')
  if (!validConfig(config)) return facadeOutput(st, 'facade', 'adapter:config_invalid')
  if (!componentAuthorized(config)) return facadeOutput(st, 'facade', 'adapter:component_pin_mismatch')
  if (!evidence) return facadeOutput(st, 'facade', 'evidence:absent')

  if (!deps.integration || typeof deps.integration.request !== 'function' || typeof deps.integration.register !== 'function' || typeof deps.integration.beforeCleanup !== 'function') throw new Error('integration missing');

  // Step 5.
  let T
  try {
    T = fileIO.mkdtempSync(path.join(os.tmpdir(), 'c3-adapter-'))
  } catch {
    return facadeOutput(st, 'facade', 'adapter:workdir_failed')
  }
  let requestBytes
  let writeStage = 'evidence-write'
  try {
    fileIO.writeFileSync(path.join(T, 'evidence.bin'), evidence, { flag: 'wx', mode: 0o600 })
    writeStage = 'request-binding'
    const request = { schema: REQUEST_SCHEMA, admission_operation_id: st.operationId, component_dir: componentDir, config, integration: deps.integration.request(T, componentDir, config) }
    writeStage = 'request-serialize'
    requestBytes = Buffer.from(serializeJson(request), 'utf8')
    writeStage = 'request-write'
    fileIO.writeFileSync(path.join(T, 'request.json'), requestBytes, { flag: 'wx', mode: 0o600 })
  } catch (error) {
    operational('adapter:workdir_failed', writeStage, error)
    st.workdirCleanup = (deps.integration.beforeCleanup(T, run.exit, observed()), removeWorkdir(T))
    return facadeOutput(st, 'facade', 'adapter:workdir_failed')
  }
  st.requestSha = sha256hex(requestBytes)
  deps.integration.register(T, componentDir, config, st.requestSha)

  // Step 6.
  let child
  try {
    child = deps.spawn(config.python.path, ['-I', '-P', '-B', path.join(componentDir, 'py', 'front.py'), T], {
      shell: false, detached: true, env: {}, stdio: ['ignore', 'pipe', 'pipe'],
    })
  } catch (error) {
    operational('adapter:front_spawn_failed', 'spawn', error)
    child = null
  }
  if (!child || child.pid === undefined) {
    if (child) {
      child.on('error', () => {})
      for (const s of [child.stdout, child.stderr]) if (s) s.on('error', () => {})
    }
    st.workdirCleanup = (deps.integration.beforeCleanup(T, run.exit, observed()), removeWorkdir(T))
    return facadeOutput(st, 'facade', 'adapter:front_spawn_failed')
  }
  const pid = child.pid
  run.spawned = true

  // Latched at spawn, synchronously, before any await (v6 section 5).
  const waiters = []
  const wake = () => { while (waiters.length) waiters.shift()() }
  let settleStarted = false
  let resolveSettled
  const settled = new Promise(resolve => { resolveSettled = resolve })
  const probe = () => {
    try {
      deps.kill(-pid, 0)
      return 'ok'
    } catch (e) {
      return e && typeof e.code === 'string' ? e.code : 'other'
    }
  }
  const killGroup = () => { try { deps.kill(-pid, 'SIGKILL') } catch {} }

  async function settle(sentAtStart) {
    const deadline = clock() + D_SETTLE_MS
    let sent = sentAtStart
    let group = 'unconfirmed'
    for (;;) {
      const r = probe()
      if (r === 'ESRCH') { group = sent ? 'killed' : 'gone'; break }
      if (r === 'ok' && !sent) { killGroup(); sent = true }
      const remaining = deadline - clock()
      if (remaining <= 0) break
      await delay(Math.min(PROBE_MS, remaining))
    }
    if (!run.closed) {
      const remaining = deadline - clock()
      if (remaining > 0) {
        await new Promise(resolve => {
          const timer = setTimer(done, remaining)
          waiters.push(done)
          function done() { clearTimer(timer); resolve() }
        })
      }
    }
    const closeSeen = run.closed
    if (!closeSeen) {
      run.destroyedByFacade = true
      child.stdout.destroy()
      child.stderr.destroy()
    }
    return { group, closeSeen }
  }

  function trigger(kind) {
    if (settleStarted) return
    settleStarted = true
    const killNow = kind === 'timeout' || kind === 'bound'
    if (killNow) killGroup()
    settle(killNow).then(resolveSettled)
  }

  const runTimer = setTimer(() => {
    if (run.exit === null) { run.timeoutAt = clock(); operational('adapter:front_timeout','timeout'); trigger('timeout') }
  }, D_RUN_MS)

  child.on('error', () => {})
  child.on('exit', (code, signal) => {
    clearTimer(runTimer)
    run.exit = { code, signal }
    trigger('exit')
  })
  child.on('close', () => { run.closed = true; wake() })
  for (const [name, bound] of [['stdout', STDOUT_BOUND], ['stderr', STDERR_BOUND]]) {
    const stream = child[name]
    stream.on('data', chunk => {
      if (run[name + 'Truncated']) return
      const room = bound - run[name + 'Bytes']
      if (chunk.length > room) {
        run[name].push(chunk.subarray(0, room))
        run[name + 'Bytes'] += room
        run[name + 'Truncated'] = true
        if (run.boundAt === null) { run.boundAt = clock(); operational('adapter:front_output_bound','capture-bound') }
        trigger('bound')
      } else {
        run[name].push(chunk)
        run[name + 'Bytes'] += chunk.length
      }
    })
    stream.on('error', e => {
      if (!run.destroyedByFacade && run[name + 'Error'] === null) {
        run[name + 'Error'] = errorCode(e)
        operational('adapter:front_capture_error',name+'-capture',e)
      }
    })
  }

  const { group, closeSeen } = await settled
  clearTimer(runTimer)
  st.processCleanup = !closeSeen ? 'unconfirmed' : group === 'gone' ? 'complete' : group === 'killed' ? 'killed' : 'unconfirmed'
  st.workdirCleanup = (deps.integration.beforeCleanup(T, run.exit, observed()), removeWorkdir(T))

  // Steps 7-14, first failure wins (v6 section 6).
  if (run.timeoutAt !== null || run.boundAt !== null) {
    const timeoutFirst = run.boundAt === null || (run.timeoutAt !== null && run.timeoutAt <= run.boundAt)
    return facadeOutput(st, 'facade', timeoutFirst ? 'adapter:front_timeout' : 'adapter:front_output_bound')
  }
  if (run.stdoutError !== null || run.stderrError !== null) return facadeOutput(st, 'facade', 'adapter:front_capture_error')
  if (run.exit === null) return facadeOutput(st, 'facade', 'adapter:front_unsettled')
  if (run.exit.signal !== null) return facadeOutput(st, 'facade', `adapter:front_signal:${run.exit.signal}`)
  if (run.exit.code !== 0) {
    // P0 holds only when steps 7 and 7b passed (addendum P-2); the no-stream-error condition is redundant here.
    const p0 = run.exit.code === 4 && closeSeen && run.stdoutBytes === 0 && run.stderrBytes === 0 &&
      run.stdoutError === null && run.stderrError === null
    return p0 ? facadeOutput(st, 'front', 'adapter:front_input_invalid') : facadeOutput(st, 'facade', `adapter:front_exit:${run.exit.code}`)
  }
  if (!closeSeen) return facadeOutput(st, 'facade', 'adapter:front_unsettled')
  if (run.stderrBytes > 0) return facadeOutput(st, 'facade', 'adapter:front_stderr_nonempty')
  const stdout = Buffer.concat(run.stdout)
  let doc
  try {
    doc = parseFrontLine(stdout)
  } catch {
    return facadeOutput(st, 'facade', 'adapter:front_output_invalid')
  }
  if (!validateFrontOutput(doc, { operationId: st.operationId, evidenceSha: st.evidenceSha, requestSha: st.requestSha })) {
    return facadeOutput(st, 'facade', 'adapter:front_output_invalid')
  }
  run.accepted = true
  if (st.processCleanup === 'killed') return facadeOutput(st, 'facade', 'adapter:descendant_survived')
  if (st.processCleanup === 'unconfirmed') return facadeOutput(st, 'facade', 'adapter:process_cleanup_unconfirmed')
  if (st.workdirCleanup === 'remove_failed') return facadeOutput(st, 'facade', 'adapter:workdir_cleanup_failed')
  return {
    evidence: new Uint8Array(stdout.subarray(0, stdout.length - 1)),
    claims: [{ claim: doc.claim.claim, status: doc.claim.status, reason: doc.claim.reason }],
  }
}

export function makeAdapter(ctx, deps) {
  const manifest = deps.manifest || JSON.parse(fs.readFileSync(path.join(componentDir, 'manifest.json'), 'utf8'))
  const config = ctx ? ctx.config : undefined
  return {
    describe: () => structuredClone(manifest),
    check: input => runCheck(config, input, deps),
  }
}

export function createAdapter(ctx) {
  return makeAdapter(ctx, { spawn: childProcess.spawn, kill: process.kill.bind(process), integration: ctx && ctx.integration })
}
