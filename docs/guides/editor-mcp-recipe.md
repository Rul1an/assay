# Editor MCP Recipe: policy-enforcing MCP in Claude Code, Cursor, Codex

Coding agents are MCP clients. You can put Assay between the agent and the MCP
servers it uses by wrapping each server with `assay mcp wrap`, so every tool call is
checked against your policy inline. No bespoke plugin is needed; you only change the
server command in the agent's standard MCP config.

## Install the Claude Code plugin

The marketplace plugin packages Assay's five MCP review tools and the
`assay-golden-path` skill. It does **not** package the server binary, invoke a target
tool, or enforce another MCP server by itself.

From the project where Claude Code should use Assay:

```bash
# 1. Install the local stdio server prerequisite.
cargo install assay-mcp-server --locked

# 2. Add Assay's marketplace and install the plugin for this project.
claude plugin marketplace add Rul1an/assay
claude plugin install assay@assay --scope local

# 3. Verify that the installed plugin can start the server.
claude mcp list
```

Restart Claude Code after installation, then ask it to use
`assay:assay-golden-path`. The skill carries its contract and the three fixtures it
needs from the isolated plugin cache. Python is optional for steps 1-5; only the
protected-action simulation in step 6 runs the bundled Python fixture.

The plugin starts `assay-mcp-server --policy-root .`. The `.` is resolved from the
working directory supplied by the host; project scope does not by itself prove that
the host selected the project root. If it did not, override the Assay entry in
project MCP configuration with an explicit absolute `--policy-root`; do not edit
the installed plugin cache.

### Update and inspect stale state

Plugin updates require a restart to take effect:

```bash
claude plugin marketplace update assay
claude plugin update assay@assay --scope local
claude plugin list --json
```

The JSON listing exposes the installed cache version and path. If an update still
shows old bytes, remove and reinstall `assay@assay` rather than modifying the cache.

### Diagnose the layer that failed

| Observation | Layer | Next step |
|---|---|---|
| `assay@assay` is absent from `claude plugin list --json` | Plugin installation | Add/update the `assay` marketplace, then install the local-scope plugin. |
| `claude mcp list` reports a spawn or command failure | Binary prerequisite | Run `command -v assay-mcp-server`, install it on `PATH`, then restart Claude Code. |
| The server connects but reports a missing policy | Project policy root | Start Claude Code from the project or configure an explicit absolute `--policy-root`. |
| `assay_policy_decide` returns `allowed=false` | Assay policy verdict | Inspect the matched rule and change the proposed action or the reviewed policy. This is not an installation failure. |

A missing plugin, failed process spawn, or unavailable policy is never a clean Assay
verdict. The server is local stdio and needs no network or transport authentication,
but installing it does not prove provider execution or external side effects.

Maintainers can exercise the bounded, disposable installation contract without
touching their normal Claude configuration:

```bash
bash scripts/ci/test-claude-plugin-install.sh --self-test
```

## Install Assay's review tools

The repository ships project configuration for the five tools exposed by the
standalone `assay-mcp-server` binary. Install that binary on `PATH`, then open the
repository in your client:

```bash
cargo install --path crates/assay-mcp-server --locked
```

- Claude Code reads `.mcp.json` from the repository root.
- Cursor reads `.cursor/mcp.json`.
- Codex users can add the equivalent entry to project `.codex/config.toml` or
  user `~/.codex/config.toml`:

```toml
[mcp_servers.assay]
command = "assay-mcp-server"
args = ["--policy-root", "."]
```

The server is local stdio and needs no network or transport authentication. It
evaluates policy and trace inputs supplied to its tools; it does not invoke or
enforce the target MCP tool call. `--policy-root .` resolves policy paths against
the server process's working directory. If a host uses another directory, set an
explicit local policy root in that client's uncommitted user or project
configuration.

The release build exposes `assay_check_args`, `assay_check_sequence`,
`assay_policy_decide`, `assay_check_coverage`, and `assay_explain_trace`.
`assay_test_outbound` is test-feature-only and is not part of the release surface.
Plain stdio mode exposes these review tools; it does not imply the separate
`proxy-enforce` mode is active.

The rest of this guide covers a different surface: wrapping a real MCP server so
Assay can enforce its tool calls at the protocol boundary.

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
