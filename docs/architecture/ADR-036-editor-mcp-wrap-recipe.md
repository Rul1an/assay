# ADR-036: Editor MCP Wrap Recipe

## Status
Proposed (June 2026; remote/OAuth section finalises after the 28 July 2026 MCP spec)

Depends on ADR-034 (Assay / Runner / Harness contract seam).

## Context

Coding agents (Claude Code, Cursor, Codex) are MCP clients and use standard MCP
client configuration as their extension point. Assay ships `assay mcp wrap`, which
wraps a real MCP server process and enforces policy decisions inline, plus
`assay mcp config-path` to locate the active config. There is no documented recipe
for inserting Assay into an agent's MCP setup.

The MCP specification finalising on 28 July 2026 brings a stateless HTTP core, server
UIs rendered in a sandboxed iframe (every UI action routed through the same JSON-RPC
audit and consent path as a direct tool call), a Tasks extension, and authorization
aligned to OAuth 2.1 / OIDC (PKCE, scoped tokens, consent).

## Decision

Provide a one-page recipe to route an agent's MCP servers through
`assay mcp wrap --policy <file> -- <real-server> [args]` using each host's standard
MCP client config: Claude Code project config, Cursor `.cursor/mcp.json`, and Codex
`AGENTS.md` / MCP config. Recommend `--dry-run` first, then enforce. Use standard MCP
client config; do not build a bespoke per-editor plugin.

Write the recipe against the 2026-07-28 release candidate. For remote servers, align
the wrapped server to the OAuth 2.1 flow with least-privilege scopes and consent;
mark that section provisional and finalise it once the spec is final. The local
stdio wrap is stable and is the primary path.

## Consequences

- Any MCP-client coding agent can run its tool calls through an Assay policy gate
  with a one-line config change. Low cost (docs + a quickstart).
- The recipe must track the 2026-07-28 spec and each host's MCP config conventions.

## Best-practice basis (2026)

- MCP 2026-07-28 spec: OAuth 2.1 / OIDC authorization, sandboxed-iframe UIs, unified
  audit and consent path.
- MCP security guidance: least privilege, run servers sandboxed with restricted
  filesystem and network, explicit privilege grants.

## Non-claims

- `assay mcp wrap` enforces policy at the MCP protocol boundary (which tools, which
  arguments). It is the protocol-level complement to kernel-level containment, not a
  replacement, and not a prompt-injection defense.

## References

- `docs/reference/cli/mcp-server.md`
- `docs/guides/editor-mcp-recipe.md`
- ADR-034 (contract seam)
