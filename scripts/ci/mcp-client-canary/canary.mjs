// MCP client canary for `assay mcp wrap`.
//
// Drives wrap with the official TypeScript client in both negotiation modes, `legacy` (the 2025
// handshake) and `auto` (2026-07-28 when the server offers it), against two cases:
//
//   recipe-pins    the Filesystem walkthrough in docs/guides/editor-mcp-recipe.md, with the
//                  policy, label and server packages the recipe names;
//   changing-list  a pinned tool while the server changes its tool list and announces it.
//
// A client upgrade that changes what a host sends, or an Assay change that stops reading it,
// turns a case red. This is an Assay compatibility check with pinned packages, not a conformance
// claim about MCP, the client or the server.
//
// Usage: node scripts/ci/mcp-client-canary/canary.mjs --binary head [--binary release]
//        node scripts/ci/mcp-client-canary/canary.mjs --recipe-release
// Each binary has one fixed place: `head` is the checkout's build (target/debug/assay), `release`
// is the release the workflow installs for this run (target/assay-release/assay), which must
// report the release the recipe names. `--recipe-release` prints that release, so CI installs
// exactly what a reader is told to. No executable path comes from the command line.
// Exit: 0 every expectation held, 1 an expectation failed, 2 the harness could not run a case.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../../..');
const RECIPE = path.join(ROOT, 'docs/guides/editor-mcp-recipe.md');
const HEAD_BINARY = path.join(ROOT, 'target/debug/assay');
const RELEASE_BINARY = path.join(ROOT, 'target/assay-release/assay');
const MODES = ['legacy', 'auto'];
const ERA = { legacy: 'legacy', auto: 'modern' };
// `auto` must land on the revision this canary documents; `legacy` on a 2025 revision.
const REVISION = { legacy: /^2025-\d\d-\d\d$/, auto: /^2026-07-28$/ };

class HarnessError extends Error {}

function binaries() {
  const found = [];
  for (let i = 2; i < process.argv.length; i += 2) {
    const flag = process.argv[i];
    const label = process.argv[i + 1];
    if (flag !== '--binary') throw new HarnessError(`unexpected argument ${flag}`);
    if (label === 'head') found.push({ label: 'head', path: HEAD_BINARY });
    else if (label === 'release') found.push({ label: 'release', path: RELEASE_BINARY });
    else throw new HarnessError('--binary takes head or release');
  }
  if (!found.length) throw new HarnessError('give --binary head and/or --binary release');
  if (new Set(found.map((b) => b.label)).size !== found.length) throw new HarnessError('each binary at most once');
  return found;
}

function only(matches, what) {
  if (matches.length !== 1) throw new HarnessError(`the recipe must contain exactly one ${what}, found ${matches.length}`);
  return matches[0];
}

// What the recipe tells a reader to run is the input, so a recipe edit moves the canary with it.
function readRecipe() {
  const text = fs.readFileSync(RECIPE, 'utf8');
  const policy = only([...text.matchAll(/cat > policy\.yaml <<'YAML'\n([\s\S]*?)\nYAML\n/g)], 'policy block')[1] + '\n';
  const label = only([...text.matchAll(/'--label', '([^']+)'/g)], 'wrap label')[1];
  const install = only(text.split('\n').filter((line) => line.includes('@modelcontextprotocol/server-filesystem@')), 'server install line');
  const packages = Object.fromEntries([...install.matchAll(/(@?[\w./-]+)@([0-9][\w.-]*)/g)].map((m) => [m[1], m[2]]));
  const link = only([...text.matchAll(/^\[Assay ([0-9.]+) CLI\]\(https:\/\/github\.com\/Rul1an\/assay\/releases\/tag\/v([0-9.]+)\)/gm)], 'release link');
  const check = only([...text.matchAll(/^assay version +# Check that this prints ([0-9.]+) /gm)], 'version check');
  const claims = new Set([link[1], link[2], check[1]]);
  if (claims.size !== 1) throw new HarnessError(`the recipe names more than one release: ${[...claims]}`);
  // The arguments the recipe passes the Filesystem server, such as `--confine-roots`, as it writes them.
  const tail = only([...text.matchAll(/path\.resolve\('node_modules\/@modelcontextprotocol\/server-filesystem\/dist\/index\.js'\),\n([\s\S]*?)\n\];/g)], 'server argument list')[1];
  const serverArgs = tail.split(',').map((token) => token.trim()).filter(Boolean).map((token) => {
    const literal = /^'([^']*)'$/.exec(token);
    if (literal) return { literal: literal[1] };
    if (token === "fs.realpathSync('files')") return { files: true };
    throw new HarnessError(`the recipe passes a server argument the canary cannot reproduce: ${token}`);
  });
  return { policy, label, packages, serverArgs, release: link[1] };
}

function checkPackages(recipe, failures) {
  for (const [name, wanted] of Object.entries(recipe.packages)) {
    const manifest = path.join(HERE, 'node_modules', name, 'package.json');
    if (!fs.existsSync(manifest)) throw new HarnessError(`${name} is not installed; run npm ci in ${HERE}`);
    const installed = JSON.parse(fs.readFileSync(manifest, 'utf8')).version;
    if (installed !== wanted) failures.push(`package ${name} is ${installed}, the recipe names ${wanted}`);
  }
}

function scratch(name) {
  return fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), `assay-canary-${name}-`)));
}

// One client session through wrap. Server frames are recorded as received, before the SDK's
// schema parsing removes members it does not model, such as wrap's `tool_identity`.
async function session({ assay, mode, dir, label, serverArgv, roots }) {
  const args = ['mcp', 'wrap', '--policy', path.join(dir, 'policy.yaml'), '--label', label,
    '--event-source', 'assay://ci/mcp-client-canary', '--decision-log', path.join(dir, 'decisions.ndjson'),
    '--decision-log-new', '--', process.execPath, ...serverArgv];
  const transport = new StdioClientTransport({ command: assay, args, cwd: dir, stderr: 'pipe' });
  const stderr = [];
  transport.stderr?.on('data', (chunk) => stderr.push(chunk));
  const frames = [];
  let receive;
  Object.defineProperty(transport, 'onmessage', {
    configurable: true,
    get() { return receive && ((message, extra) => { frames.push(message); return receive(message, extra); }); },
    set(handler) { receive = handler; },
  });
  let refreshed = 0;
  const client = new Client({ name: 'assay-mcp-client-canary', version: '0.0.0' }, {
    capabilities: roots ? { roots: {} } : {},
    versionNegotiation: { mode, probe: { timeoutMs: 15000 } },
    listChanged: { tools: { autoRefresh: true, onChanged: () => { refreshed += 1; } } },
  });
  if (roots) client.setRequestHandler('roots/list', async () => ({ roots: [{ uri: pathToFileURL(roots).href }] }));
  try {
    await client.connect(transport, { timeout: 30000 });
  } catch (error) {
    throw new HarnessError(`connect (${mode}) failed: ${error?.message ?? error}; wrap stderr: ${Buffer.concat(stderr).toString().slice(-400)}`);
  }
  const call = async (name, args = {}) => {
    try {
      const result = await client.callTool({ name, arguments: args });
      return { isError: result.isError === true, text: (result.content ?? []).map((c) => c.text ?? '').join(' ') };
    } catch (error) {
      return { isError: true, text: `request error: ${error?.message ?? error}` };
    }
  };
  const waitForRefresh = async () => {
    for (let i = 0; i < 50 && refreshed === 0; i += 1) await new Promise((r) => setTimeout(r, 100));
    return refreshed;
  };
  const close = async () => {
    try { await client.close(); } catch { /* the decision log is read after close either way */ }
    await new Promise((r) => setTimeout(r, 300));
    const log = path.join(dir, 'decisions.ndjson');
    const rows = fs.existsSync(log)
      ? fs.readFileSync(log, 'utf8').trim().split('\n').filter(Boolean).map((line) => JSON.parse(line).data)
      : [];
    return rows.map((d) => ({ tool: d.tool, decision: d.decision, reason: d.reason_code, digest: d.tool_definition_digest ?? null }));
  };
  return { era: client.getProtocolEra(), revision: client.getNegotiatedProtocolVersion(), listTools: () => client.listTools(),
    call, waitForRefresh, close, frames };
}

function expect(failures, label, condition, detail) {
  if (!condition) failures.push(`${label}: ${detail}`);
}

function negotiated(failures, label, s, mode) {
  expect(failures, label, s.era === ERA[mode], `negotiated era ${s.era}, expected ${ERA[mode]}`);
  expect(failures, label, REVISION[mode].test(String(s.revision)), `negotiated revision ${s.revision}, expected ${REVISION[mode]}`);
}

// The tool lists the client received, as relayed by wrap (with `tool_identity`).
function listings(frames) {
  return frames.map((f) => f.result?.tools).filter(Array.isArray);
}

function rowsAre(failures, label, rows, expected) {
  const got = rows.map((r) => `${r.tool} ${r.decision} ${r.reason}`);
  expect(failures, label, JSON.stringify(got) === JSON.stringify(expected),
    `decisions ${JSON.stringify(got)}, expected ${JSON.stringify(expected)}`);
}

async function recipePins(bin, recipe, mode, failures) {
  const label = `${bin.label}: recipe-pins/${mode}`;
  const dir = scratch('recipe');
  const files = path.join(dir, 'files');
  fs.mkdirSync(files);
  fs.writeFileSync(path.join(files, 'note.txt'), 'hello\n');
  fs.writeFileSync(path.join(dir, 'policy.yaml'), recipe.policy);
  const server = path.join(HERE, 'node_modules/@modelcontextprotocol/server-filesystem/dist/index.js');
  const serverArgv = [server, ...recipe.serverArgs.map((a) => (a.files ? fs.realpathSync(files) : a.literal))];
  const s = await session({ assay: bin.path, mode, dir, label: recipe.label, serverArgv, roots: dir });
  negotiated(failures, label, s, mode);
  // Before the client lists, a pinned tool has no observed definition: the recipe's pin must refuse it.
  const unlisted = await s.call('list_directory', { path: files });
  await s.listTools();
  const first = await s.call('list_directory', { path: files });
  const write = await s.call('write_file', { path: path.join(files, 'new.txt'), content: 'x\n' });
  const second = await s.call('list_directory', { path: files });
  // The client offers the parent as a root; the recipe's server arguments must keep the server out of it.
  const parent = await s.call('list_directory', { path: dir });
  const rows = await s.close();
  expect(failures, label, unlisted.isError, `list_directory before listing was not refused: ${JSON.stringify(unlisted)}`);
  expect(failures, label, !first.isError && first.text.includes('note.txt'), `first listing ${JSON.stringify(first)}`);
  expect(failures, label, write.isError, `write_file was not refused: ${JSON.stringify(write)}`);
  expect(failures, label, !second.isError && second.text.includes('note.txt') && !second.text.includes('new.txt'), `second listing ${JSON.stringify(second)}`);
  // Only the Filesystem server's own refusal of that path counts; a transport or other error does not.
  expect(failures, label, parent.isError && parent.text.startsWith(`Access denied - path outside allowed directories: ${dir} `),
    `the server did not refuse the parent directory as outside its allowed directories: ${JSON.stringify(parent)}`);
  rowsAre(failures, label, rows, ['list_directory deny P_TOOL_UNOBSERVED', 'list_directory allow E_TOOL_UNCONSTRAINED',
    'write_file deny P_TOOL_NOT_ALLOWED', 'list_directory allow E_TOOL_UNCONSTRAINED', 'list_directory allow E_TOOL_UNCONSTRAINED']);
  expect(failures, label, fs.readdirSync(files).join(',') === 'note.txt', `files after: ${fs.readdirSync(files)}`);
  const digests = rows.filter((r) => r.tool === 'list_directory' && r.decision === 'allow').map((r) => r.digest);
  expect(failures, label, digests.length === 3 && digests.every((d) => d && d === digests[0]), `list_directory digests ${JSON.stringify(digests)}`);
  return digests[0] ?? null;
}

// The pin is taken from what wrap reports for an unpinned 2025 listing of the same server.
async function changingPin(bin) {
  const dir = scratch('pin');
  fs.writeFileSync(path.join(dir, 'policy.yaml'), 'version: "2.0"\nname: canary\ntools:\n  allow: [snapshot, mutate, tabs]\n');
  const s = await session({ assay: bin.path, mode: 'legacy', dir, label: 'changing', serverArgv: [path.join(HERE, 'changing-server.mjs'), 'tabs'] });
  await s.listTools();
  await s.close();
  const listed = s.frames.map((f) => f.result?.tools?.find((t) => t.name === 'snapshot')).find(Boolean);
  const identity = listed?.tool_identity;
  if (!identity?.schema_hash || !identity?.meta_hash) throw new HarnessError('wrap reported no tool_identity for snapshot in a 2025 listing');
  return 'version: "2.0"\nname: canary\ntools:\n  allow: [snapshot, mutate, tabs]\ntool_pins:\n  snapshot:\n'
    + `    server_id: changing\n    tool_name: snapshot\n    schema_hash: ${identity.schema_hash}\n    meta_hash: ${identity.meta_hash}\n`;
}

// What changing-server.mjs says after `mutate`, per changed tool.
const CHANGED = { tabs: 'List tabs, changed.', snapshot: 'Take a snapshot and upload it.' };

async function changingList(bin, policy, mode, target, failures) {
  const label = `${bin.label}: changing-list/${target}/${mode}`;
  const other = target === 'tabs' ? 'snapshot' : 'tabs';
  const dir = scratch('changing');
  fs.writeFileSync(path.join(dir, 'policy.yaml'), policy);
  const s = await session({ assay: bin.path, mode, dir, label: 'changing', serverArgv: [path.join(HERE, 'changing-server.mjs'), target] });
  negotiated(failures, label, s, mode);
  await s.listTools();
  await s.call('snapshot');
  await s.call('mutate');
  const refreshed = await s.waitForRefresh();
  await s.call('snapshot');
  const rows = await s.close();
  expect(failures, label, refreshed > 0, 'the client never refreshed its tool list after the change');
  // The refreshed list must carry the change: the changed tool's description and wrap's identity for it
  // move, and the other tool stays as it was. A change that does not happen cannot pass this case.
  const lists = listings(s.frames);
  const find = (list, name) => list?.find((tool) => tool.name === name);
  const before = lists[0];
  const after = lists.at(-1);
  expect(failures, label, lists.length >= 2, `expected a listing before and after the change, got ${lists.length}`);
  expect(failures, label, find(after, target)?.description === CHANGED[target] && find(before, target)?.description !== CHANGED[target],
    `${target} description ${JSON.stringify(find(before, target)?.description)} -> ${JSON.stringify(find(after, target)?.description)}`);
  expect(failures, label, find(before, target)?.tool_identity?.meta_hash && find(after, target)?.tool_identity?.meta_hash
    && find(before, target).tool_identity.meta_hash !== find(after, target).tool_identity.meta_hash,
    `wrap's identity for ${target} did not move with its definition`);
  expect(failures, label, JSON.stringify(find(before, other)) === JSON.stringify(find(after, other)), `${other} changed as well`);
  rowsAre(failures, label, rows, ['snapshot allow E_TOOL_UNCONSTRAINED', 'mutate allow E_TOOL_UNCONSTRAINED',
    target === 'tabs' ? 'snapshot allow E_TOOL_UNCONSTRAINED' : 'snapshot deny P_TOOL_DRIFT']);
}

const failures = [];

async function main() {
  if (process.argv[2] === '--recipe-release') {
    if (process.argv.length !== 3) throw new HarnessError('--recipe-release takes no other arguments');
    console.log(readRecipe().release);
    return 0;
  }
  const bins = binaries();
  const recipe = readRecipe();
  checkPackages(recipe, failures);
  for (const bin of bins) {
    if (!fs.existsSync(bin.path)) throw new HarnessError(`no assay binary at ${bin.path}`);
    if (bin.label === 'release') {
      const reported = execFileSync(bin.path, ['version'], { encoding: 'utf8' }).trim();
      expect(failures, 'release', reported === recipe.release,
        `the installed binary reports ${reported}, the recipe names ${recipe.release}`);
    }
    const digests = {};
    for (const mode of MODES) digests[mode] = await recipePins(bin, recipe, mode, failures);
    expect(failures, `${bin.label}: recipe-pins`, digests.legacy && digests.legacy === digests.auto,
      `list_directory digest differs between modes: ${JSON.stringify(digests)}`);
    const policy = await changingPin(bin);
    for (const target of ['tabs', 'snapshot']) {
      for (const mode of MODES) await changingList(bin, policy, mode, target, failures);
    }
  }
  for (const failure of failures) console.log(`FAIL ${failure}`);
  const ran = bins.map((b) => b.label).join(', ');
  console.log(failures.length ? `mcp-client-canary: ${failures.length} expectation(s) failed (${ran})`
    : `mcp-client-canary: all expectations held for ${ran} (recipe-pins, changing-list; legacy and auto)`);
  return failures.length ? 1 : 0;
}

main().then((code) => { process.exitCode = code; }, (error) => {
  for (const failure of failures) console.log(`FAIL ${failure}`);
  console.log(`mcp-client-canary: harness error: ${error?.message ?? error}`);
  process.exitCode = 2;
});
