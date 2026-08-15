# MCP Outer Fallback Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the built-in stdio MCP policy tools fail closed on dispatch errors and timeouts, remove caller-selected fail-open, and publish only fixed bounded diagnostics.

**Architecture:** Normalize timeout and handler failures into one `ToolError`-backed value before logging, decision evidence, and `CallToolResult` wrapping. The stdio server has no fail-open mode: suite-level `on_error` remains an `assay run` concern, while caller-supplied `arguments.on_error` has no authority. Keep the existing MCP 2025-era response shape and the landed 4,096-byte `ToolError` serialization bound; do not add configuration, an error-mode enum, schema validation, or 2026-07-28 fields.

**Tech Stack:** Rust 2024, Tokio, serde/serde_json, MCP JSON-RPC over stdio, existing `tests/jsonrpc_conn` harness, MkDocs Markdown.

## Global Constraints

- Base SHA: `d1bfd36a11b857222d9c43f291a8011eaeee23e6`.
- The stdio MCP server always fails closed for handler errors and timeouts: `allowed:false`, `isError:true`.
- `arguments.on_error` never changes server behavior and remains absent from every advertised input schema.
- Reuse `ToolError` serialization; do not add a second message ceiling or sanitizer.
- Unknown methods retain JSON-RPC `-32601`, but their public message is fixed and value-free.
- Unknown tools remain an MCP `CallToolResult` in this slice; moving them to JSON-RPC `-32602` is a separate protocol-shape decision.
- Do not change proxy enforcement, authentication, policy parsing, policy ingest, tool schemas, `resultType`, or `structuredContent`.
- Every commit stages explicit paths only.

---

### Task 1: Freeze the hostile stdio contract

**Files:**
- Create: `crates/assay-mcp-server/tests/outer_fallback_contract.rs`
- Reuse: `crates/assay-mcp-server/tests/jsonrpc_conn/mod.rs`

**Interfaces:**
- Consumes: built `CARGO_BIN_EXE_assay-mcp-server`, `Conn`, `ASSAY_MCP_TIMEOUT_MS`.
- Produces: exact wire assertions for `allowed`, `isError`, code, message, warning absence, response shape, and full-response sentinel absence.

- [ ] **Step 1: Write the failing real-stdio tests**

Add three tests using a fresh temporary `--policy-root` and a valid legacy `initialize` request:

```rust
#[test]
fn caller_cannot_fail_open_handler_errors() {
    // Drive assay_check_args with missing required fields once with no on_error and once with
    // {"on_error":"allow"}. Both must publish the same fixed E_INTERNAL failure:
    // allowed=false, isError=true, message="Tool execution failed", no warning.
}

#[test]
fn unknown_names_are_value_free() {
    // Use one unknown tool/method string containing HEAD_SENTINEL, MID_SENTINEL, and
    // TAIL_SENTINEL separated by long UTF-8 content. Assert none appears anywhere in each
    // serialized response. Unknown tool is fixed E_INTERNAL; unknown method is fixed -32601.
}

#[test]
fn caller_cannot_fail_open_timeouts() {
    // Spawn with ASSAY_MCP_TIMEOUT_MS=1 and call assay_check_args with policy slow.yaml,
    // including on_error=allow. Assert E_TIMEOUT, allowed=false, isError=true,
    // message="Tool execution timed out", and no warning.
}
```

The helper that decodes `CallToolResult.content[0].text` must assert the complete key semantics rather than only checking the error code.

- [ ] **Step 2: Run the tests and record RED**

Run:

```bash
cargo test --locked -p assay-mcp-server --test outer_fallback_contract -- --nocapture
```

Expected RED on the base:

- caller `on_error:allow` produces `allowed:true / isError:false`;
- unknown tool/method responses contain all three sentinels;
- timeout fail-open produces no warning and reports success;
- handler/timeout messages are not the fixed target strings.

- [ ] **Step 3: Commit only the RED contract**

```bash
git add -- crates/assay-mcp-server/tests/outer_fallback_contract.rs
git commit -m "test(mcp): freeze outer fallback authority contract"
```

### Task 2: Normalize every outer failure through one fail-closed path

**Files:**
- Modify: `crates/assay-mcp-server/src/server.rs:315-533`
- Test: `crates/assay-mcp-server/tests/outer_fallback_contract.rs`

**Interfaces:**
- Consumes: `tools::ToolError::new(...).result()` and existing `ServerConfig.timeout_ms`.
- Produces: private `fail_closed_tool_result(code, message) -> anyhow::Result<Value>` and one common `CallToolResult` wrapper.

- [ ] **Step 1: Add the single publication helper**

In `server.rs`, add:

```rust
fn fail_closed_tool_result(code: &'static str, message: &'static str) -> Result<Value> {
    tools::ToolError::new(code, message).result()
}
```

This helper contains no truncation logic. `ToolError` remains the only message-boundary implementation.

- [ ] **Step 2: Remove caller authority and normalize dispatch outcomes**

Delete the `args.get("on_error")` read and every derived `allow_on_error` branch. Restructure the timeout result into one `Value` before common logging and wrapping:

```rust
let tool_result = match timeout(Duration::from_millis(cfg.timeout_ms), fut).await {
    Ok(Ok(value)) => value,
    Ok(Err(_error)) => {
        tracing::error!(event = "tool_execution_error", rid = %rid, code = "E_INTERNAL");
        fail_closed_tool_result("E_INTERNAL", "Tool execution failed")?
    }
    Err(_) => {
        tracing::warn!(event = "tool_call_timeout", rid = %rid, code = "E_TIMEOUT");
        fail_closed_tool_result("E_TIMEOUT", "Tool execution timed out")?
    }
};
```

Do not publish `_error` or include caller-controlled values in the outer-fallback
`tool_call_*` telemetry. The separate pre-existing `tool_decision` evidence event keeps its own
redaction contract. Preserve non-sensitive timing and request-id telemetry. Feed `tool_result`
through one classification function before emitting evidence and wrapping the MCP result:

```rust
let (allowed, is_error) = classify_tool_result(&tool_result);
```

An explicit `allowed: false` is both a denial and an MCP tool error. A payload with `error` is an
MCP tool error. Report tools without either field keep their pre-existing non-allow decision
telemetry but return a successful MCP result.

Use the fixed JSON-RPC message `"Method not found"` for the unknown-method branch. Do not add a fail-open warning because no fail-open state remains.

- [ ] **Step 3: Run GREEN and adjacent suites**

```bash
cargo test --locked -p assay-mcp-server --test outer_fallback_contract -- --nocapture
cargo test --locked -p assay-mcp-server --test stdio_edge_cases -- --nocapture
cargo test --locked -p assay-mcp-server --test policy_diagnostic_safety -- --nocapture
cargo test --locked -p assay-mcp-server tools::tests -- --nocapture
```

Expected: all pass; stderr remains structured and does not contain the hostile sentinels.

- [ ] **Step 4: Run distinct mutation checks**

On disposable copies, prove the contract fails independently when:

1. caller `arguments.on_error` controls `allowed` again;
2. handler `e.to_string()` is published again;
3. timeout returns `allowed:true`;
4. unknown method interpolates `req.method`;
5. `isError` is hardcoded false for an error result;
6. the fallback bypasses `ToolError` and emits an over-ceiling test message.

Record each failing test name and panic/assertion; restore the worktree from the committed paths, never with a destructive whole-tree command.

- [ ] **Step 5: Commit the minimal production change**

```bash
git add -- crates/assay-mcp-server/src/server.rs
git commit -m "fix(mcp): fail closed at the outer tool boundary"
```

### Task 3: Correct outward failure-policy documentation

**Files:**
- Modify: `docs/concepts/fail-safe.md`
- Modify: `docs/guides/gateway-pattern.md`
- Modify: `docs/reference/cli/mcp-server.md`
- Modify: `docs/reference/mcp-api.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: the Task 2 wire contract.
- Produces: one truthful distinction between batch-suite `on_error` and stdio MCP dispatch behavior.

- [ ] **Step 1: Correct the conceptual boundary**

State explicitly that suite/test/assertion `on_error` applies to `assay run`. Replace the streaming-mode table with the shipped stdio invariant: tool execution errors and timeouts fail closed, and caller `arguments.on_error` is not an authority.

Remove the fictional `policy_check_error / on_error_mode` audit sample. Describe only current machine-readable results and structured stderr events, without inventing field names.

- [ ] **Step 2: Replace the gateway fail-open recipe**

Delete the caller-controlled JSON example and the recommendation to begin production deployment fail-open. Document availability without bypass: bounded client retry, supervised restart, redundant instances where appropriate, and alerting on `E_TIMEOUT` / `E_INTERNAL`. Do not claim the built-in stdio server forwards target actions; it returns policy decisions only.

- [ ] **Step 3: Pin the CLI/API contract and migration**

Document that:

- `assay-mcp-server` always returns `allowed:false / isError:true` for outer failures;
- `arguments.on_error` is ignored and is absent from advertised schemas;
- users who followed the old gateway recipe must remove that argument;
- `settings.on_error` remains supported by `assay run` and is not an MCP server setting;
- unknown tool remains a tool result in this slice, while unknown method is `-32601`.

Add an `[Unreleased]` changelog entry naming #2391 and the intentional security-hardening compatibility change.

- [ ] **Step 4: Verify docs and commit**

```bash
pre-commit run --files \
  CHANGELOG.md \
  docs/concepts/fail-safe.md \
  docs/guides/gateway-pattern.md \
  docs/reference/cli/mcp-server.md \
  docs/reference/mcp-api.md
git diff --check
git add -- \
  CHANGELOG.md \
  docs/concepts/fail-safe.md \
  docs/guides/gateway-pattern.md \
  docs/reference/cli/mcp-server.md \
  docs/reference/mcp-api.md
git commit -m "docs(mcp): document fail-closed dispatch semantics"
```

### Task 4: Final verification and review packet

**Files:**
- Verify all files changed in Tasks 1-3.

**Interfaces:**
- Produces: a clean exact-head commit, draft PR, primary-source-linked review packet, and an independent non-building review request.

- [ ] **Step 1: Run the full affected verification**

```bash
cargo test --locked -p assay-mcp-server
cargo fmt --all -- --check
cargo clippy --locked -p assay-mcp-server --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked -p assay-mcp-server --no-deps
git diff --check origin/main...HEAD
git status --short
```

- [ ] **Step 2: Inspect public strings and final scope**

```bash
rg -n 'on_error|FAIL-SAFE ACTIVE|e\.to_string\(\)|Method not found:' \
  crates/assay-mcp-server/src/server.rs \
  docs/concepts/fail-safe.md \
  docs/guides/gateway-pattern.md \
  docs/reference/cli/mcp-server.md \
  docs/reference/mcp-api.md
git diff --stat origin/main...HEAD
git diff --name-only origin/main...HEAD
```

Expected: no caller authority, no raw error publication, no caller interpolation, and only the planned paths.

- [ ] **Step 3: Push and open a draft PR**

Push `codex/2391-outer-fallback`, open a draft PR referencing #2391, and include:

- exact base/head SHA and worktree;
- RED/GREEN and mutation witnesses;
- real-stdio before/after smoking-gun rows;
- MCP 2025-06-18 error taxonomy and OWASP least-agency/complete-mediation rationale;
- compatibility note for the removed gateway recipe;
- explicit non-claims from Global Constraints.

- [ ] **Step 4: Obtain exact-head independent review**

Claude's main chat and Cursor participated in the design and do not count. Ask Claude to spawn a fresh subagent with no access to this design conversation, or use another non-building reviewer. Merge only after the exact-head review is READY, all actionable findings are disposed, and all required contexts are green.

## Self-Review

- Spec coverage: authority inversion, raw diagnostic leak, timeout behavior, caller/method reflection, UTF-8 bound reuse, migration, docs, and non-claims each map to a task.
- Placeholder scan: no TBD/TODO or unspecified implementation step remains.
- Type consistency: the plan adds only `fail_closed_tool_result(&'static str, &'static str) -> anyhow::Result<Value>` and otherwise reuses existing `ToolError`, `Value`, and `Conn` interfaces.
- Simplicity check: rejected env/CLI knobs, a second error-policy enum, precedence logic, schema validation, and a new sanitizer because no measured requirement justifies them.
