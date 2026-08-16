# MCP Runtime Commands

This page documents the current MCP runtime entry points in Assay.

---

## 1) `assay mcp` (CLI)

`assay mcp` is the canonical command family for MCP runtime work:

- `assay mcp wrap`
- `assay mcp preflight`
- `assay mcp discover`
- `assay mcp kill`
- `assay mcp config-path`
- `assay mcp tool keygen|sign|verify`

The previous flat `assay discover`, `assay kill`, and `assay tool ...`
spellings remain available as hidden compatibility shims and print a
deprecation warning to stderr. Their stdout, exit codes, artifacts, and output
formats remain unchanged.

---

## 2) `assay mcp wrap` (CLI)

Wrap a real MCP process and enforce policy decisions inline.

### Synopsis

```bash
assay mcp wrap [OPTIONS] -- <command> [args...]
```

### Common Usage

```bash
# Enforcing mode
assay mcp wrap --policy assay.yaml -- <real-mcp-command> [args...]

# Dry-run mode (log decisions, do not block)
assay mcp wrap --policy assay.yaml --dry-run -- <real-mcp-command> [args...]
```

### Key Options

| Option | Description |
|--------|-------------|
| `--policy <PATH>` | Policy file (default: `assay.yaml`) |
| `--dry-run` | Log decisions but do not block |
| `--verbose` | Print decisions to stderr |
| `--label <LABEL>` | Logical server label for identity tracking |
| `--audit-log <PATH>` | Write mandate lifecycle events (requires `--event-source`) |
| `--decision-log <PATH>` | Write decision events (requires `--event-source`) |
| `--event-source <URI>` | CloudEvents source URI, e.g. `assay://org/app` |
| `-- <command> [args...]` | Wrapped process (required) |

---

## 3) `assay mcp preflight` (CLI)

Check whether `assay-mcp-server` on PATH matches this CLI and can start with a
policy root. This command does not start an MCP session for a host.

### Synopsis

```bash
assay mcp preflight [--policy-root .] [--format terminal|json]
```

### Options

| Option | Description |
|--------|-------------|
| `--policy-root <PATH>` | Directory the host would pass as `--policy-root` (default: `.`). Must exist and be a directory. |
| `--format <terminal\|json>` | Report format (default: `terminal`). JSON is one `assay.mcp_preflight.v0` object. |

Exit `0` only for `ready`. Every other phase exits `2`.

Both child probes use a 2-second wall-clock deadline and cap each of stdout
and stderr at 8 KiB before materialization. Child output is never copied into
the preflight JSON document.

### JSON document (`assay.mcp_preflight.v0`)

`--format json` prints one object. These fields are always present:

| Field | Meaning |
|--------|---------|
| `schema` | Always `assay.mcp_preflight.v0`. |
| `phase` | One of `missing`, `unstartable`, `wrong_version`, `invalid_root`, `startup_refused`, `startup_timeout`, `ready`. |
| `message` | Stable diagnosis for that phase. |
| `next_step` | Recovery string; empty only for `ready`. |
| `expected_version` | This CLI's `CARGO_PKG_VERSION`. |
| `policy_root` | The path that was checked. |

`actual_version` is present exactly when the identity probe parsed a version
token. Identity-time `missing` and `unstartable` omit it. The same phases
reached after the second spawn retain it, as do `wrong_version`,
`invalid_root`, `startup_refused`, `startup_timeout`, and `ready`.

`ready` proves only a matching identity followed by an accepted startup
attempt. It does not prove both resolutions selected one immutable
executable.

This is a command-local schema. It is not a `ReasonCode`, policy verdict, or
host-discovery document.

---

## 4) `assay-mcp-server` (separate binary)

Run the MCP server binary directly.

### Synopsis

```bash
assay-mcp-server --policy-root <DIR>
```

### Key Options

| Option | Description |
|--------|-------------|
| `--policy-root <PATH>` | Policy root directory (default: `policies`) |

### Outer failure contract

Tool dispatch failures and timeouts always fail closed. The returned tool
payload has `allowed: false`; the MCP `CallToolResult` has `isError: true`.
Dispatch failures use `E_INTERNAL` with `Tool execution failed`, and timeouts
use `E_TIMEOUT` with `Tool execution timed out`.

`arguments.on_error` is not an operator control and is absent from the
advertised input schemas. Remove it from clients that followed an older gateway
example. `settings.on_error` remains available to `assay run`; it is not a
server setting.

Unknown methods remain JSON-RPC `-32601` with the fixed message `Method not
found`. Unknown tool names currently return the fail-closed `CallToolResult`;
this does not claim JSON-RPC `-32602` routing for unknown tools.

### Policy-file ingest ceiling

The five advertised policy tools (`assay_policy_decide`, `assay_check_args`,
`assay_check_sequence`, `assay_check_coverage`, `assay_explain_trace`) read
local policy files through one inclusive byte ceiling before parse or cache
insertion. The default is 1,000,000 bytes. Override it with
`ASSAY_MCP_MAX_POLICY_BYTES`. Exactly the configured size is accepted; one
extra byte returns `E_LIMIT_EXCEEDED` and is not parsed or cached.

This ceiling is independent of `ASSAY_MCP_MAX_BYTES`, which bounds inbound
JSON-RPC messages. It does not limit YAML nesting, alias expansion, proxy
startup policy, declared manifests, trust policy, or CLI policy readers.

Operators whose local policy files previously exceeded 1,000,000 bytes must
either reduce the file or set an explicit bounded override.

---

## Agent Integration Note

For agent-side runtime enforcement, prefer `assay mcp wrap` so the wrapped MCP process is mediated by Assay policy checks.

See also:
- [MCP Integration](../../mcp/index.md)
- [Self-Correction Guide](../../mcp/self-correction.md)
- [Policies](../../concepts/policies.md)
