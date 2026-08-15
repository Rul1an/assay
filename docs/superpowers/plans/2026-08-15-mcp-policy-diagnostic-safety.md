# MCP Policy Diagnostic Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close #2387 by replacing source-derived public policy-parse messages with stable,
value-free summaries and by enforcing one 4,096-byte UTF-8-safe ceiling at every `ToolError`
serialization boundary.

**Architecture:** Keep tool failures as MCP tool results. Add one private `bound_public_message`
function in `tools/mod.rs`, use it in both `ToolError::new` and a manual `Serialize` implementation,
and add one policy-parse constructor that accepts only a fixed failure class plus optional numeric
location. Migrate the four raw parser sinks; leave the already-fixed sequence diagnostic on the
general constructor.

**Tech Stack:** Rust 1.96, serde/serde_json/serde_yaml, real stdio JSON-RPC tests, pre-commit,
GitHub Actions.

## Global Constraints

- Baseline is the `main` SHA containing #2386; record it before creating the branch.
- Implement in one isolated `claude/2387-policy-diagnostic-safety` worktree owned by Claude.
- Public parse summaries are exactly `Policy YAML is invalid`, `Policy root must be a mapping`, or
  `Policy structure is invalid`; do not append parser text.
- The 4,096-byte ceiling applies to serialized `error.message`, not the complete MCP envelope.
- Truncation must preserve valid UTF-8 and must not add a suffix beyond the ceiling.
- Optional location is numeric `details.line` and `details.column`; no source line, path, or error
  chain enters `message` or `details`.
- Keep `ToolError` and its fields public for semver compatibility.
- Do not absorb the outer server's separately constructed `E_INTERNAL` fallback or caller-selected
  `on_error`; #2391 owns that generic dispatch boundary.
- Do not change verdict semantics, reason codes, JSON-RPC errors, policy readers, or cache order.
- Stage only named paths; never use a whole-tree staging command.

---

## File Structure

- Modify `crates/assay-mcp-server/src/tools/mod.rs`: shared bound, parse failure class, custom serialization.
- Modify `crates/assay-mcp-server/src/tools/policy_decide.rs`: value-free parser errors.
- Modify `crates/assay-mcp-server/src/tools/check_args.rs`: value-free full-policy parser errors.
- Modify `crates/assay-mcp-server/src/tools/check_coverage.rs`: value-free YAML parser errors.
- Modify `crates/assay-mcp-server/src/tools/explain_trace.rs`: value-free YAML parser errors.
- Add `crates/assay-mcp-server/tests/policy_diagnostic_safety.rs`: real-stdio hostile-input contract.

### Task 1: Freeze the public diagnostic boundary

**Files:**
- Add: `crates/assay-mcp-server/tests/policy_diagnostic_safety.rs`
- Modify: `crates/assay-mcp-server/src/tools/mod.rs` (unit tests only in this task)

- [ ] **Step 1: Add a real-stdio helper covering four raw sinks**

Spawn `CARGO_BIN_EXE_assay-mcp-server` with a temporary `--policy-root`, initialize protocol
`2024-11-05`, call `assay_policy_decide`, `assay_check_args`, `assay_check_coverage`, and
`assay_explain_trace`, and decode `result.content[0].text` as JSON. Every assertion must inspect the
complete response string as well as `/error/message`.

- [ ] **Step 2: Add three hostile sentinel positions**

Create separate malformed policies with unique `BEGIN_SECRET`, `MIDDLE_SECRET`, and `END_SECRET`
sentinels embedded in a 200,000-byte invalid scalar. For each tool assert:

```text
result.isError == true
body.allowed == false
body.error.code == E_POLICY_PARSE
body.error.message is one fixed summary
body.error.message.as_bytes().len() <= 4096
complete response contains none of the three sentinels
```

Use distinct files or requests so one failure cannot mask another position.

- [ ] **Step 3: Add path and location controls**

For malformed syntax at a known line/column, assert `details.line` and `details.column` are positive
integers when that parser supplies location. Assert the complete response excludes the canonical
temporary root and policy path. Add a malformed multibyte input whose bounded message must remain
valid UTF-8.

- [ ] **Step 4: Pin constructor and serializer bypasses**

In `tools/mod.rs` tests serialize all three forms with a message longer than 4,096 bytes:

```rust
ToolError::new("E_TEST", &message)
ToolError { code: "E_TEST".into(), message: message.clone(), details: None }
let mut error = ToolError::new("E_TEST", "short"); error.message = message;
```

Assert serialized `/message` is valid UTF-8 and at most 4,096 bytes. The direct construction and
post-construction mutation are load-bearing: they prove the publication boundary, not only the
constructor, applies the rule.

- [ ] **Step 5: Run RED**

```bash
cargo test --locked -p assay-mcp-server --test policy_diagnostic_safety -- --nocapture
cargo test --locked -p assay-mcp-server tools::tests -- --nocapture
```

Expected: hostile parser values appear in responses and direct/mutated `ToolError` serialization is
unbounded. Record the exact failures in #2388.

- [ ] **Step 6: Commit only RED tests**

```bash
git add -- \
  crates/assay-mcp-server/tests/policy_diagnostic_safety.rs \
  crates/assay-mcp-server/src/tools/mod.rs
git commit -m "test(mcp): expose unbounded policy diagnostics"
```

### Task 2: Implement one bounded publication boundary

**Files:**
- Modify: `crates/assay-mcp-server/src/tools/mod.rs`

- [ ] **Step 1: Add the UTF-8-safe bound**

Define `const MAX_PUBLIC_MESSAGE_BYTES: usize = 4096` and one function:

```rust
fn bound_public_message(message: &str) -> &str {
    if message.len() <= MAX_PUBLIC_MESSAGE_BYTES {
        return message;
    }
    let mut end = MAX_PUBLIC_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    &message[..end]
}
```

Do not duplicate this character-boundary rule in constructor and serializer.

- [ ] **Step 2: Replace derived serialization**

Remove `#[derive(serde::Serialize)]` from `ToolError`. Implement `serde::Serialize` manually through
a private serializable view that borrows `code`, the bounded message slice, and `details`. Preserve
the existing field names and `skip_serializing_if` behavior.

- [ ] **Step 3: Make the constructor call the same function**

`ToolError::new` stores `bound_public_message(message).to_owned()`. Keep the serializer bound too;
public fields can bypass or mutate constructor state.

- [ ] **Step 4: Add a fixed parse constructor**

Add a private or `pub(crate)` enum with only the three approved classes and a constructor such as:

```rust
pub(crate) fn policy_parse(
    class: PolicyParseFailure,
    location: Option<(usize, usize)>,
) -> Self
```

Map the enum to fixed summaries. Populate `details` only as `{"line": line, "column": column}`.
Do not accept a raw parser `Display` string.

- [ ] **Step 5: Run the serialization tests GREEN**

```bash
cargo test --locked -p assay-mcp-server tools::tests -- --nocapture
```

- [ ] **Step 6: Commit the shared boundary**

```bash
git add -- crates/assay-mcp-server/src/tools/mod.rs
git commit -m "fix(mcp): bound public tool error messages"
```

### Task 3: Migrate every raw policy-parse sink

**Files:**
- Modify: `crates/assay-mcp-server/src/tools/policy_decide.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_args.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_coverage.rs`
- Modify: `crates/assay-mcp-server/src/tools/explain_trace.rs`

- [ ] **Step 1: Inventory and pin the baseline**

```bash
rg -n 'E_POLICY_PARSE|Failed to parse policy|e\.to_string\(\)' \
  crates/assay-mcp-server/src/tools
```

Record every hit in the PR body. `check_sequence` may retain `Invalid sequence policy format`
because it is fixed and value-free, but it must not become a second parse-policy constructor.

- [ ] **Step 2: Map syntax and structure without raw text**

For `serde_yaml::Error`, extract only `location().map(|loc| (loc.line(), loc.column()))` and pass it
to `ToolError::policy_parse`. Use `YamlSyntax` where syntax decoding fails and `Structure` where a
typed mapping has the wrong shape. In `policy_decide`, retain #2386's explicit root class.

- [ ] **Step 3: Handle `check_args` without widening into the reader/parser slice**

Keep the existing not-found/read branches as a separate concern until #2389 centralizes them. Map
the current fall-through parse/validation error to `PolicyParseFailure::Structure` without
publishing `e.to_string()`. Do not add a bytes/string `McpPolicy` parser here: extracting that
single parse rule, preserving file/bytes parity, and removing `from_file` from this callsite are the
explicit work of #2389.

- [ ] **Step 4: Run GREEN and inspect public text**

```bash
cargo test --locked -p assay-mcp-server --test policy_diagnostic_safety -- --nocapture
cargo test --locked -p assay-mcp-server --test policy_decide_blocklist -- --nocapture
cargo test --locked -p assay-mcp-server --test stdio_edge_cases -- --nocapture
rg -n 'E_POLICY_PARSE.*(e\.to_string|format!)|Failed to parse policy' \
  crates/assay-mcp-server/src/tools
```

Expected: tests pass and the grep finds no source-derived public parse sink.

- [ ] **Step 5: Run discriminating mutations**

In disposable copies, separately: return a raw parser string from one sink; remove the constructor
bound; remove the serializer bound; and replace the UTF-8 boundary loop with a byte slice. Each
mutation must fail a different sentinel, direct-struct, post-mutation, or multibyte assertion.

- [ ] **Step 6: Commit migrated sinks and integration test**

```bash
git add -- \
  crates/assay-mcp-server/src/tools/policy_decide.rs \
  crates/assay-mcp-server/src/tools/check_args.rs \
  crates/assay-mcp-server/src/tools/check_coverage.rs \
  crates/assay-mcp-server/src/tools/explain_trace.rs \
  crates/assay-mcp-server/tests/policy_diagnostic_safety.rs
git commit -m "fix(mcp): normalise public policy parse diagnostics"
```

### Task 4: Verify and publish #2387

- [ ] **Step 1: Run required local verification**

```bash
cargo test --locked -p assay-mcp-server --test policy_diagnostic_safety -- --nocapture
cargo test --locked -p assay-mcp-server
cargo fmt --all -- --check
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
git diff --check
git status --short
```

- [ ] **Step 2: Inspect scope and strings**

```bash
git diff --stat origin/main...HEAD
rg -n 'E_POLICY_PARSE|Policy (YAML|root|structure)' crates/assay-mcp-server/src/tools
```

Confirm no absolute path, source fragment, parser implementation text, verdict change, or reader
limit entered this slice.

- [ ] **Step 3: Push, open the PR, and record the ledger**

Push `claude/2387-policy-diagnostic-safety`, open against current `main`, and record branch, PR,
exact head SHA, tests, mutations, non-claims, and findings in #2388.

- [ ] **Step 4: Obtain exact-head quorum**

Use one fresh non-building reviewer who authored neither the governing spec/plan nor the code. Any
push invalidates the review. Merge only after required checks, exact-head review, and delegated
proof (when triggered) all bind to the final SHA.
