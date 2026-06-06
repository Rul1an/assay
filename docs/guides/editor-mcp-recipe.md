# Editor MCP Recipe: policy-enforcing MCP in Claude Code, Cursor, Codex

Coding agents are MCP clients. You can put Assay between the agent and the MCP
servers it uses by wrapping each server with `assay mcp wrap`, so every tool call is
checked against your policy inline. No bespoke plugin is needed; you only change the
server command in the agent's standard MCP config.

## The wrap command

```bash
assay mcp wrap --policy assay.yaml -- <real-mcp-server-command> [args...]
```

Key options:

| Option | Effect |
|--------|--------|
| `--policy <PATH>` | Policy file (default `assay.yaml`) |
| `--dry-run` | Log decisions, do not block (start here) |
| `--verbose` | Print decisions to stderr |

Recommended path: run with `--dry-run` first to see decisions, then drop it to
enforce.

## Claude Code

In your project MCP config, set the server's command to the wrapped form:

```json
{
  "mcpServers": {
    "files": {
      "command": "assay",
      "args": ["mcp", "wrap", "--policy", "assay.yaml", "--",
               "<real-mcp-server>", "<server-args>"]
    }
  }
}
```

## Cursor

In `.cursor/mcp.json`, same shape:

```json
{
  "mcpServers": {
    "files": {
      "command": "assay",
      "args": ["mcp", "wrap", "--policy", "assay.yaml", "--",
               "<real-mcp-server>", "<server-args>"]
    }
  }
}
```

## Codex

In your `AGENTS.md` / Codex MCP config, register the same wrapped command as the
server entry. Use `assay mcp config-path` to locate the active config.

## Remote servers (provisional, MCP 2026-07-28)

The MCP specification finalising on 28 July 2026 aligns remote authorization with
OAuth 2.1 / OIDC (PKCE, scoped tokens, consent), and renders server UIs in a
sandboxed iframe with every UI action going through the same audit and consent path
as a direct tool call. For remote MCP servers, align the wrapped server to that
OAuth 2.1 flow and keep scopes least-privilege. This section is provisional against
the release candidate and will be finalised once the spec is final; the local
stdio wrap above is stable today.

## Honest limits

- `assay mcp wrap` enforces policy at the MCP protocol boundary (which tools, which
  arguments). It is the protocol-level complement to kernel-level containment, not a
  replacement for it, and not a prompt-injection defense.
- Least privilege still applies: scope the wrapped server's filesystem and network
  access, and grant more only when needed.

See also: [Coding-Agent Governance](coding-agent-governance.md), [ADR-036](../architecture/ADR-036-editor-mcp-wrap-recipe.md).
