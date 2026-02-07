# MCP Runtime Commands

This page documents the current MCP runtime entry points in Assay.

---

## 1) `assay mcp wrap` (CLI)

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

## 2) `assay-mcp-server` (separate binary)

Run the MCP server binary directly.

### Synopsis

```bash
assay-mcp-server --policy-root <DIR>
```

### Key Options

| Option | Description |
|--------|-------------|
| `--policy-root <PATH>` | Policy root directory (default: `policies`) |

---

## Agent Integration Note

For agent-side runtime enforcement, prefer `assay mcp wrap` so the wrapped MCP process is mediated by Assay policy checks.

See also:
- [MCP Integration](../../mcp/index.md)
- [Self-Correction Guide](../../mcp/self-correction.md)
- [Policies](../../concepts/policies.md)
