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
location. Direct YAML consumers use one two-stage value/root/typed helper; the full-policy parser
tags syntax, root, shape, and validation failures at source for `check_args`. Migrate the four raw parser
sinks; leave the already-fixed sequence diagnostic on the general constructor.

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
- Do not change verdict semantics, reason codes, JSON-RPC errors, reader paths/bounds, or cache
  order. The full-policy reader may switch from `read_to_string` to bytes only to classify invalid
  UTF-8 as parse input while preserving ordinary file-I/O behavior.
- Stage only named paths; never use a whole-tree staging command.

---

## File Structure

- Modify `crates/assay-mcp-server/src/tools/mod.rs`: shared bound, parse failure class, custom serialization.
- Modify `crates/assay-mcp-server/src/tools/policy_decide.rs`: value-free parser errors.
- Modify `crates/assay-mcp-server/src/tools/check_args.rs`: value-free full-policy parser errors.
- Modify `crates/assay-mcp-server/src/tools/check_coverage.rs`: value-free YAML parser errors.
- Modify `crates/assay-mcp-server/src/tools/explain_trace.rs`: value-free YAML parser errors.
- Add `crates/assay-mcp-server/tests/policy_diagnostic_safety.rs`: real-stdio hostile-input contract.
- Modify `crates/assay-core/src/mcp/policy/mod.rs`: expose the typed full-policy failure kind.
- Modify `crates/assay-core/src/mcp/policy/legacy.rs`: tag syntax, root, shape, and validation failures at source.
- Modify `crates/assay-core/src/mcp/tests.rs`: pin full-policy error classification.
- Add `crates/assay-core/tests/mcp_policy_warning_contract.rs`: subprocess warning/ordering contract.
- Add `scripts/ci/check-mcp-policy-yaml-routing.py`: enforce the single mapping/parser boundary.
- Add `scripts/ci/test-check-mcp-policy-yaml-routing.sh`: mutation-test the routing guard.
- Add `scripts/ci/check-mcp-tool-error-publication.py` and its self-test: bind `result` to `Serialize`.
- Modify `.pre-commit-config.yaml`: run the guard for every guarded parser or consumer.

### Task 1: Freeze the public diagnostic boundary

**Files:**
- Add: `crates/assay-mcp-server/tests/policy_diagnostic_safety.rs`
- Modify: `crates/assay-mcp-server/src/tools/mod.rs` (unit tests only in this task)
- Modify: `crates/assay-core/src/mcp/tests.rs` (tests only in this task)
- Add: `crates/assay-core/tests/mcp_policy_warning_contract.rs`
- Modify: `crates/assay-core/src/mcp/policy/mod.rs` (interface-only typed seam)
- Modify: `crates/assay-core/src/mcp/policy/legacy.rs` (interface-only typed seam)

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
body.error.message equals the summary for the exercised failure class
body.error.message.as_bytes().len() <= 4096
complete response contains none of the three sentinels
```

Use distinct files or requests so one failure cannot mask another position.
Include malformed syntax that must return `Policy YAML is invalid`, a well-formed mapping with a
typed field-shape error that must return `Policy structure is invalid`, and the non-mapping control
from #2386 that must return `Policy root must be a mapping`. Do not let one generic fixed-summary
assertion stand in for these three classes.

- [ ] **Step 3: Add path and location controls**

For malformed syntax at a known line/column, assert `details.line` and `details.column` are positive
integers when that parser supplies location. Assert the complete response excludes the canonical
temporary root and policy path. Add a malformed multibyte input whose bounded message must remain
valid UTF-8. Separately construct a long message whose byte 4,096 falls inside a multibyte code
point; reuse that exact message in every serializer-bypass case below.

Add a separate policy file containing invalid UTF-8 bytes and exercise `assay_check_args`. Assert
the public class is `E_POLICY_PARSE` with exactly `Policy YAML is invalid`, not `E_POLICY_READ`,
`E_INTERNAL`, or an OS/UTF-8 diagnostic.

- [ ] **Step 4: Freeze successful full-policy semantics and warnings**

Before changing `legacy::from_file`, add independent controls for V2 tools/schema, legacy root
allow/deny normalization, V1 constraint migration, validation failure, unknown-field warning,
non-strict V1 deprecation warning, and strict-deprecations rejection. Pin normalized results and
warning behavior, not parser strings.

Test the process-global deprecation warning in a dedicated integration-test binary. A parent test
spawns `current_exe()` with an ignored exact child and no-timestamp tracing; the child parses one V1
fixture with an unknown field twice. Assert unknown-field warning precedes deprecation warning, the
deprecation text occurs exactly once, and the child succeeds. A child process resets `OnceLock`;
never mutate it in-process. Run these controls GREEN on the baseline before continuing.

- [ ] **Step 5: Pin constructor and serializer bypasses**

In `tools/mod.rs` tests serialize all three forms with that same boundary-bisecting message:

```rust
ToolError::new("E_TEST", &message)
ToolError { code: "E_TEST".into(), message: message.clone(), details: None }
let mut error = ToolError::new("E_TEST", "short"); error.message = message;
```

For the constructor case, inspect `error.message` before serialization and assert it equals the
exact UTF-8-safe prefix and is at most 4,096 bytes; removing the constructor bound must fail here
even if serialization still bounds. Then serialize all three forms and assert `/message` is the
same valid UTF-8 bounded prefix. Finally publish the direct and post-construction-mutated structs
through `ToolError::result` and assert `/error/message` is bounded. If `result` repacks public fields
instead of serializing `ToolError`, those tests must fail. The three paths separately prove the
constructor, serializer, and result publication boundary.

- [ ] **Step 6: Add an interface-only classifier seam**

Declare the public non-exhaustive `McpPolicyErrorKind` and typed wrapper, but do not yet make the
parser construct it. This changes no parser result; it only lets the core classification test
compile and downcast the existing `anyhow::Error`. The test must then fail its kind assertions rather
than fail compilation.

- [ ] **Step 7: Run baseline controls and behavioral RED**

```bash
cargo test --locked -p assay-mcp-server --test policy_diagnostic_safety -- --nocapture
cargo test --locked -p assay-mcp-server tools::tests -- --nocapture
cargo test --locked -p assay-core mcp::tests::policy_error_classification -- --nocapture
cargo test --locked -p assay-core mcp::tests::policy_file_parser_contract -- --nocapture
cargo test --locked -p assay-core --test mcp_policy_warning_contract -- --nocapture
```

Expected: hostile parser values appear in responses and direct/mutated `ToolError` serialization is
unbounded; typed downcasts fail because the parser does not construct the seam; invalid UTF-8 still
publishes `E_POLICY_PARSE` but leaks the raw UTF-8 diagnostic and lacks the typed `Syntax` kind.
Successful parser/warning controls remain green. Record the exact assertion failures in #2388. A
compile failure does not count as RED evidence.

- [ ] **Step 8: Commit the interface seam, behavior freeze, and RED tests**

```bash
git add -- \
  crates/assay-mcp-server/tests/policy_diagnostic_safety.rs \
  crates/assay-mcp-server/src/tools/mod.rs \
  crates/assay-core/src/mcp/tests.rs \
  crates/assay-core/tests/mcp_policy_warning_contract.rs \
  crates/assay-core/src/mcp/policy/mod.rs \
  crates/assay-core/src/mcp/policy/legacy.rs
git commit -m "test(mcp): expose unbounded policy diagnostics"
```

### Task 2: Implement one bounded publication boundary

**Files:**
- Modify: `crates/assay-mcp-server/src/tools/mod.rs`
- Add: `scripts/ci/check-mcp-tool-error-publication.py`
- Add: `scripts/ci/test-check-mcp-tool-error-publication.sh`
- Modify: `.pre-commit-config.yaml`

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

- [ ] **Step 4: Guard the result publication edge**

Add a structural guard that identifies `ToolError::result`, requires it to serialize `self` through
the `ToolError` `Serialize` implementation, and forbids rebuilding `error` from public fields or a
second bounded view. Its disposable self-test must fail both an unbounded repack and a faithful
bounded repack that calls `bound_public_message` but bypasses `Serialize`. Wire it to pre-commit for
`tools/mod.rs`, the guard, and its self-test.

- [ ] **Step 5: Add a fixed parse constructor**

Add a private or `pub(crate)` enum with only the three approved classes and a constructor such as:

```rust
pub(crate) fn policy_parse(
    class: PolicyParseFailure,
    location: Option<(usize, usize)>,
) -> Self
```

Map the enum to fixed summaries. Populate `details` only as `{"line": line, "column": column}`.
Do not accept a raw parser `Display` string.

- [ ] **Step 6: Run the serialization tests and guard GREEN**

```bash
cargo test --locked -p assay-mcp-server tools::tests -- --nocapture
python3 scripts/ci/check-mcp-tool-error-publication.py
bash scripts/ci/test-check-mcp-tool-error-publication.sh
```

- [ ] **Step 7: Commit the shared boundary**

```bash
git add -- \
  crates/assay-mcp-server/src/tools/mod.rs \
  scripts/ci/check-mcp-tool-error-publication.py \
  scripts/ci/test-check-mcp-tool-error-publication.sh \
  .pre-commit-config.yaml
git commit -m "fix(mcp): bound public tool error messages"
```

### Task 3: Migrate every raw policy-parse sink

**Files:**
- Modify: `crates/assay-core/src/mcp/policy/mod.rs`
- Modify: `crates/assay-core/src/mcp/policy/legacy.rs`
- Modify: `crates/assay-core/src/mcp/tests.rs`
- Modify: `crates/assay-mcp-server/src/tools/mod.rs`
- Modify: `crates/assay-mcp-server/src/tools/policy_decide.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_args.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_coverage.rs`
- Modify: `crates/assay-mcp-server/src/tools/explain_trace.rs`
- Add: `scripts/ci/check-mcp-policy-yaml-routing.py`
- Add: `scripts/ci/test-check-mcp-policy-yaml-routing.sh`
- Modify: `.pre-commit-config.yaml`

- [ ] **Step 1: Inventory and pin the baseline**

```bash
rg -n 'E_POLICY_PARSE|Failed to parse policy|e\.to_string\(\)' \
  crates/assay-mcp-server/src/tools
```

Record every hit in the PR body. `check_sequence` may retain `Invalid sequence policy format`
because it is fixed and value-free, but it must not become a second parse-policy constructor.

- [ ] **Step 2: Map syntax and structure without raw text**

Add one generic direct-tool helper in `tools/mod.rs` around a shared mapping-stage function: decode
bytes to `serde_yaml::Value`, require a mapping root, and return the mapping-stage value plus the
approved syntax/root classification. The generic helper then deserializes that value into the
tool's typed policy and classifies typed failure as `Structure`; extract only safe numeric location
from the first-stage error. `check_coverage` and `explain_trace` must call the generic helper.
`policy_decide` must call the same mapping-stage function before its private dialect check. No
direct consumer may construct a YAML deserializer or duplicate the mapping-root rule.

- [ ] **Step 3: Handle `check_args` without widening into the reader/parser slice**

Keep not-found and ordinary file-I/O branches separate until #2389 centralizes their bounded read.
Complete the interface-only wrapper from Task 1. Change `legacy::from_file` from
`read_to_string` to `std::fs::read`: file-open/read errors remain ordinary I/O errors, while
`std::str::from_utf8` failure is wrapped as `McpPolicyErrorKind::Syntax` before YAML decode. Then
decode to `serde_yaml::Value`, check that the root is a mapping, and run the current
ignored-field-aware typed deserialize, normalization, migration, and validation sequence. Tag those
stages `Syntax`, `RootNotMapping`, `Structure`, and `Validation` respectively. Never classify by
`Display`. `check_args` maps `Syntax` to
`PolicyParseFailure::YamlSyntax`, `RootNotMapping` to `PolicyParseFailure::RootNotMapping`, and
`Structure`/`Validation` to `PolicyParseFailure::Structure`. The core test pins all four kinds,
including a scalar-root fixture.

Do not add a parallel bytes/string `McpPolicy` parser here. #2389 moves this same tagged parse rule
behind the bytes API, preserves file/bytes parity, and removes `from_file` from the `check_args`
callsite. The diagnostic slice must therefore leave one classifier for #2389 to reuse, not a second
server-side approximation.

- [ ] **Step 4: Run GREEN and inspect public text**

```bash
cargo test --locked -p assay-mcp-server --test policy_diagnostic_safety -- --nocapture
cargo test --locked -p assay-mcp-server --test policy_decide_blocklist -- --nocapture
cargo test --locked -p assay-mcp-server --test stdio_edge_cases -- --nocapture
cargo test --locked -p assay-core mcp::tests::policy_error_classification -- --nocapture
cargo test --locked -p assay-core mcp::tests::policy_file_parser_contract -- --nocapture
cargo test --locked -p assay-core --test mcp_policy_warning_contract -- --nocapture
python3 scripts/ci/check-mcp-policy-yaml-routing.py
bash scripts/ci/test-check-mcp-policy-yaml-routing.sh
rg -n 'E_POLICY_PARSE.*(e\.to_string|format!)|Failed to parse policy' \
  crates/assay-mcp-server/src/tools
```

Expected: tests pass and the grep finds no source-derived public parse sink.

- [ ] **Step 5: Run discriminating mutations**

Add and self-test a structural source guard that permits YAML parser/deserializer construction only
inside the shared mapping/generic helpers and the `assay-core` full-policy parser, and requires
`policy_decide`, `check_coverage`, and `explain_trace` to call the approved helper. Disposable
fixtures must fail when a faithful duplicate two-stage parser is inserted at a consumer. Wire it to
pre-commit for `tools/mod.rs`, `policy_decide.rs`, `check_args.rs`, `check_coverage.rs`,
`explain_trace.rs`, `policy/mod.rs`, `policy/legacy.rs`, the guard, and its self-test. Self-test a
bypass in the direct-consumer class and the full-policy-parser class.

Then, in disposable copies, separately: return a raw parser string from one sink; collapse syntax,
root, and typed-shape classes; bypass the helper; remove the constructor bound; remove the serializer
bound; and replace the UTF-8 boundary loop with a byte slice. Each mutation must fail a different
structural, sentinel, classification, direct-struct, post-mutation, or multibyte assertion.

- [ ] **Step 6: Commit migrated sinks and integration test**

```bash
git add -- \
  crates/assay-core/src/mcp/policy/mod.rs \
  crates/assay-core/src/mcp/policy/legacy.rs \
  crates/assay-core/src/mcp/tests.rs \
  crates/assay-core/tests/mcp_policy_warning_contract.rs \
  crates/assay-mcp-server/src/tools/mod.rs \
  crates/assay-mcp-server/src/tools/policy_decide.rs \
  crates/assay-mcp-server/src/tools/check_args.rs \
  crates/assay-mcp-server/src/tools/check_coverage.rs \
  crates/assay-mcp-server/src/tools/explain_trace.rs \
  crates/assay-mcp-server/tests/policy_diagnostic_safety.rs \
  scripts/ci/check-mcp-policy-yaml-routing.py \
  scripts/ci/test-check-mcp-policy-yaml-routing.sh \
  .pre-commit-config.yaml
git commit -m "fix(mcp): normalise public policy parse diagnostics"
```

### Task 4: Verify and publish #2387

- [ ] **Step 1: Run required local verification**

```bash
cargo test --locked -p assay-mcp-server --test policy_diagnostic_safety -- --nocapture
cargo test --locked -p assay-core mcp::tests::policy_error_classification -- --nocapture
cargo test --locked -p assay-core mcp::tests::policy_file_parser_contract -- --nocapture
cargo test --locked -p assay-core --test mcp_policy_warning_contract -- --nocapture
cargo test --locked -p assay-mcp-server
python3 scripts/ci/check-mcp-tool-error-publication.py
bash scripts/ci/test-check-mcp-tool-error-publication.sh
python3 scripts/ci/check-mcp-policy-yaml-routing.py
bash scripts/ci/test-check-mcp-policy-yaml-routing.sh
cargo fmt --all -- --check
cargo clippy -p assay-core -p assay-mcp-server --all-targets -- -D warnings
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
