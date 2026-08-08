# MCP Project Install Surfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish the existing five-tool `assay-mcp-server` stdio surface to Claude Code, Cursor, and Codex with one tested invocation.

**Architecture:** Two client-specific JSON locations carry an identical `mcpServers.assay` entry. The Codex guide carries the same command and arguments. One Rust integration test reads only an enum allowlist of repository files, drives the built binary over stdio, and pins the five server-emitted production tool names.

**Tech Stack:** JSON project manifests, Markdown/TOML documentation, Rust integration tests, `serde_json`, `std::process`.

## Global Constraints

- Canonical invocation: `assay-mcp-server --policy-root .` with no mode subcommand.
- Production names: `assay_check_args`, `assay_check_coverage`, `assay_check_sequence`, `assay_explain_trace`, `assay_policy_decide`.
- `assay_test_outbound` is test-only and must not be advertised.
- Do not add proxy, `SKILL.md`, plugin, authentication, network, or MCP 2026-07-28 remediation work.
- Do not accept arbitrary relative paths in the test helper; map enum variants to fixed repository paths before joining the workspace root.

---

### Task 1: Contract Test and Install Surfaces

**Files:**
- Create: `crates/assay-mcp-server/tests/project_install_surfaces.rs`
- Create: `.mcp.json`
- Create: `.cursor/mcp.json`
- Modify: `.gitignore`
- Modify: `docs/guides/editor-mcp-recipe.md`
- Modify: `docs/superpowers/specs/2026-08-08-mcp-project-install-surfaces-design.md`

**Interfaces:**
- Consumes: `CARGO_BIN_EXE_assay-mcp-server` and the existing stdio `tools/list` method.
- Produces: two identical `mcpServers.assay` entries and one equivalent Codex TOML example.

- [x] **Step 1: Write the failing integration test**

Create an enum whose variants map internally to exactly `.mcp.json`,
`.cursor/mcp.json`, and `docs/guides/editor-mcp-recipe.md`. Parse both JSON
entries, assert the exact Codex snippet, then spawn the built server with the
manifest arguments and send:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list"}
```

Sort the returned names and compare them with this literal set:

```rust
[
    "assay_check_args",
    "assay_check_coverage",
    "assay_check_sequence",
    "assay_explain_trace",
    "assay_policy_decide",
]
```

- [x] **Step 2: Run the test and verify RED**

Run:

```bash
CARGO_TARGET_DIR=/tmp/assay-target-2152 cargo test -p assay-mcp-server --test project_install_surfaces -- --nocapture
```

Expected: FAIL while reading the absent repository-root `.mcp.json`; compilation and binary startup setup must otherwise be valid.

- [x] **Step 3: Add the minimal manifests and guide section**

Both JSON files contain:

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

The guide adds this exact Codex block:

```toml
[mcp_servers.assay]
command = "assay-mcp-server"
args = ["--policy-root", "."]
```

It includes `cargo install assay-mcp-server --locked`, distinguishes the
standalone five-tool evaluator from `assay mcp wrap`, and says the server does
not invoke or enforce the target MCP tool call. `.gitignore` unignores only
`.cursor/mcp.json` inside the otherwise local `.cursor/` directory.

- [x] **Step 4: Run the focused test and verify GREEN**

Run the command from Step 2. Expected: PASS with the exact five names and exit
0 after stdin closes.

- [x] **Step 5: Run affected verification**

```bash
CARGO_TARGET_DIR=/tmp/assay-target-2152 cargo test -p assay-mcp-server
CARGO_TARGET_DIR=/tmp/assay-target-2152 cargo clippy -p assay-mcp-server --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Expected: all pass with no warnings.

- [x] **Step 6: Kill the three targeted mutations**

Against committed baseline `fdcf2c878aa0c115772064591f2faf233787c3a2`,
temporarily make each change below, run the focused test, verify the named
failure, and restore the original content before continuing:

1. Change one manifest's `args`: `Claude and Cursor entries drifted`.
2. Rename one `list_tools()` tool: `release tool surface changed`.
3. Remove `--policy-root` from the Codex block:
   `Codex guide does not carry the manifest invocation`.

After restoring all three mutations, rerun the focused test and require green.

- [x] **Step 7: Commit the implementation**

Stage only the seven paths named under **Files** above, including this plan and
the reviewed design correction, then commit with:

```bash
git commit -m "feat(mcp): publish project install manifests"
```
