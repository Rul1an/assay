# MCP Project Install Surfaces Design

Date: 2026-08-08
Issue: [#2152](https://github.com/Rul1an/assay/issues/2152)
Scope: steps 1 and 2 only

## Goal

Make the five production tools exposed by `assay-mcp-server` discoverable from
Claude Code, Cursor, and Codex in a cloned project. This publishes an existing
local stdio capability; it does not add tools or change their behavior.

## Surfaces

The repository will carry two project manifests because the clients use
different locations:

- Claude Code reads `.mcp.json` at the repository root.
- Cursor reads `.cursor/mcp.json`.
- Codex users get an equivalent `[mcp_servers.assay]` block in
  `docs/guides/editor-mcp-recipe.md`; Codex does not read either JSON manifest.

Both JSON files use the `mcpServers` wrapper and contain the same `assay`
server entry.

## Canonical Invocation

All three surfaces express the same process:

```text
assay-mcp-server --policy-root .
```

`assay-mcp-server` is a separate binary. `assay mcp` is a different CLI
command family and is not a substitute.

No server mode subcommand is supplied, so the process runs the standalone
stdio server. The `proxy`, `proxy-enforce`, and `enforcement-sarif` modes are
outside this slice.

The explicit policy root is required. The binary default is `policies/`, which
does not exist in this repository and is not a safe assumption for downstream
projects. `.` always names the server process's current directory. Treating it
as the project root assumes the host starts a project-scoped server there; if a
host chooses another working directory, tool-supplied policy paths resolve
against that directory instead.

## Manifest Shape

Each JSON manifest contains only the portable, documented stdio fields:

```json
{
  "mcpServers": {
    "assay": {
      "command": "assay-mcp-server",
      "args": ["--policy-root", "."]
    }
  }
}
```

An additional `note` field is deliberately omitted. A shipped Claude plugin
demonstrates that field, but Cursor's project-manifest documentation does not.
The server's limits belong in the adjacent guide and existing tool
descriptions rather than in an unverified cross-client extension field.

The guide will state that this standalone server:

- uses local stdio and needs no network or transport authentication;
- evaluates supplied policy and trace inputs;
- does not invoke or enforce the target MCP tool call;
- requires `assay-mcp-server` to be installed on `PATH`.

## Production Tool Contract

The normal release build must advertise these server-emitted names:

- `assay_check_args`
- `assay_check_coverage`
- `assay_check_sequence`
- `assay_explain_trace`
- `assay_policy_decide`

The test-only `assay_test_outbound` tool is behind the non-default
`test-outbound` feature and is not part of the install contract. Tests pin the
five names, not only their count, and do not pin host-specific renamespacing.

## Verification Design

A new `assay-mcp-server` integration test will fail before the manifests and
documentation exist, then prove the completed behavior:

1. Parse both JSON manifests and assert their `assay` entries are identical.
2. Assert the command and argument vector match the canonical invocation.
3. Assert the Codex TOML example carries the same command and arguments.
4. Start the built `assay-mcp-server` using the manifest arguments from the
   repository root.
5. Send a real `tools/list` request over stdio and assert the exact five
   production tool names.
6. Assert the process exits successfully after stdin closes.

The test uses `CARGO_BIN_EXE_assay-mcp-server`, so it exercises the binary built
for the same test run rather than an unrelated executable on the developer's
`PATH`.

This test does not claim full MCP 2026-07-28 conformance. Issue
[#2157](https://github.com/Rul1an/assay/issues/2157) separately tracks missing
`resultType`, `ttlMs`, and `cacheScope` fields plus deterministic tool ordering.
Those protocol gaps do not prevent a client from starting the server and
discovering its current production tools, which is the bounded contract here.

## Failure Behavior

The manifests do not fall back to another binary or server mode. If
`assay-mcp-server` is absent, the host reports a spawn/configuration failure. If
the policy root cannot be opened, server startup remains non-zero. Neither case
is presented as a working tool surface.

## Non-Goals

- No new MCP tools or tool behavior.
- No `assay mcp wrap` configuration for an upstream server.
- No `SKILL.md`; that remains blocked on #2154.
- No Claude plugin or marketplace packaging.
- No proxy, enforcement proxy, HTTP, OAuth, or authentication configuration.
- No MCP 2026-07-28 result, caching, ordering, or error-code remediation.
- No trust score, whole-action verdict, detector catalogue, or expanded claim.
