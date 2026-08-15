# MCP Policy Bounded Ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close #2389 by enforcing a configurable inclusive policy-file byte ceiling before any of
the five advertised MCP tools materialises or parses a local policy.

**Architecture:** Add `max_policy_bytes` to `ServerConfig` and one `ToolContext::read_policy_bounded`
entry point. The entry point performs secure path resolution, then calls a file-opening wrapper
inside `tokio::task::spawn_blocking`; that wrapper delegates to one generic
`LimitReader<R: Read>` primitive. It maps typed limit, not-found, and other I/O errors once. Add a
bytes-in `McpPolicy` parser and make file/string/check_args entry points delegate to it, so
full-policy semantics have one implementation.

**Tech Stack:** Rust 1.96, Tokio, `assay_common::limits`, serde YAML, tempfile/sparse files, real
stdio JSON-RPC tests, pre-commit, GitHub Actions.

## Global Constraints

- Baseline is the `main` SHA containing #2387; record it before creating the branch.
- Implement in one isolated `codex/2389-policy-bounded-ingest` worktree owned by Codex.
- Default `max_policy_bytes` is `1_000_000`; only `ASSAY_MCP_MAX_POLICY_BYTES` overrides it.
- The ceiling is inclusive: exactly limit is accepted; limit plus one is refused.
- Read through `LimitReader`; file metadata may optimize nothing and is not authoritative.
- All five advertised policy tools use the same reader entry point.
- Limit failures occur before parse and cache insertion and publish `E_LIMIT_EXCEEDED`.
- Keep every tool's existing policy dialect and verdict semantics after successful reading.
- Exclude proxy startup, declared manifests, trust policy, CLI readers, nesting depth, aliases,
  remote transport, and target-tool execution.
- Stage only named paths; never use a whole-tree staging command.

---

## File Structure

- Modify `crates/assay-mcp-server/src/config.rs`: independent policy-byte configuration.
- Modify `crates/assay-mcp-server/src/main.rs`: log the configured policy ceiling from `ServerConfig`.
- Modify `crates/assay-mcp-server/src/tools/mod.rs`: shared async bounded reader and error mapping.
- Modify five tool files under `crates/assay-mcp-server/src/tools/`: remove unbounded reads.
- Modify `crates/assay-core/src/mcp/policy/mod.rs` and `policy/legacy.rs`: one bytes-in parser.
- Modify `crates/assay-core/src/mcp/tests.rs`: file/bytes/string parser contract table.
- Verify `crates/assay-core/tests/mcp_policy_warning_contract.rs`: inherited subprocess warning contract.
- Add `crates/assay-mcp-server/tests/policy_ingest_limits.rs`: real-stdio five-tool boundary table.
- Add `scripts/ci/check-mcp-policy-reader-routing.py` and its self-test: pin `LimitReader` and all five callsites.
- Add `scripts/ci/check-mcp-policy-parser-delegation.py` and its self-test: pin all four full-policy delegation hops.
- Modify `.pre-commit-config.yaml`: wire both routing guards to all guarded source paths.
- Modify `CHANGELOG.md` and the MCP server/operator reference: document the default ceiling and override.

### Task 1: Establish the behavioral RED

**Files:**
- Add: `crates/assay-mcp-server/tests/policy_ingest_limits.rs` (black-box RED first)

- [ ] **Step 1: Add and run one compilable black-box RED**

Using only the existing public binary surface, start the server with a small
`ASSAY_MCP_MAX_POLICY_BYTES`. Call every advertised tool with a limit-plus-one policy and expect
`E_LIMIT_EXCEEDED`; the current server ignores that variable and the assertions must fail while the
test compiles and runs. In the same test, pin the already-shipped #2387 invalid-UTF-8
`assay_check_args` response as a GREEN regression control. Run and record the assertion failures:

```bash
cargo test --locked -p assay-mcp-server --test policy_ingest_limits -- --nocapture
git add -- crates/assay-mcp-server/tests/policy_ingest_limits.rs
git commit -m "test(mcp): expose unbounded policy-file ingest"
```

This is the behavioral RED required before any production interface or reader change. Later unit
cycles may initially fail to compile against an absent private seam, but those failures are not
reported as the RED proof.

### Task 2: Implement configuration and shared bounded reader

**Files:**
- Modify: `crates/assay-mcp-server/src/config.rs`
- Modify: `crates/assay-mcp-server/src/main.rs`
- Modify: `crates/assay-mcp-server/src/tools/mod.rs`
- Add: `scripts/ci/check-mcp-policy-reader-routing.py`
- Add: `scripts/ci/test-check-mcp-policy-reader-routing.sh`
- Modify: `.pre-commit-config.yaml`

- [ ] **Step 1: Test, then add, the single configuration value**

Add configuration tests immediately before the field change. Use one process-scoped mutex and
restore every changed variable before releasing it. Pin the `1_000_000` default, the independent
`ASSAY_MCP_MAX_POLICY_BYTES=1234` override, non-interference from `ASSAY_MCP_MAX_BYTES=4321`, and
invalid override fallback. Then add the field and make those tests pass.

Add `pub max_policy_bytes: usize` to `ServerConfig`, default it to `1_000_000`, and parse only
`ASSAY_MCP_MAX_POLICY_BYTES` in `from_env`. Log the value alongside existing limits in `main.rs`
without introducing a second literal.

- [ ] **Step 2: Test, then extract, the blocking primitive**

Before implementing the private primitive, add focused tests for exactly-limit, limit-plus-one,
chunked crossing, and ordinary I/O failure using an arbitrary `Read`. Classify through
`LimitExceeded::from_io`; never match rendered text. Then implement two exact layers: generic
`read_bounded<R: Read>(reader, limit)` owns `LimitReader` and byte accumulation; file-only
`read_file_bounded(path, limit)` opens `File` and immediately delegates to the generic primitive.
The async context method may call only the file wrapper. The generic layer uses:

Implement a helper using:

```rust
let mut reader = assay_common::limits::LimitReader::new(
    reader,
    limit as u64,
    assay_common::limits::LimitKind::SourceBytes,
);
let mut bytes = Vec::new();
std::io::Read::read_to_end(&mut reader, &mut bytes)?;
```

Both layers return bytes or the same typed internal read classification. Neither may preallocate
from `metadata().len()`.

- [ ] **Step 3: Test, then add, the async context entry point**

Before adding the context method, test missing file, directory read failure, exact-limit regular
file, and sparse limit-plus-one file. Avoid Unix-mode permission fixtures. Then implement:

`ToolContext::read_policy_bounded(user_path)` first calls the existing secure
`resolve_policy_path`, then moves the owned path and limit into `tokio::task::spawn_blocking`.
Centralize mapping:

```text
NotFound -> E_POLICY_NOT_FOUND with the relative request path only
LimitExceeded -> E_LIMIT_EXCEEDED with a fixed value-free message
other I/O or JoinError -> E_POLICY_READ with a fixed value-free message
```

Do not include canonical roots, OS diagnostics, or policy contents.

- [ ] **Step 4: Add and self-test the reader-routing guard**

Require the generic primitive to construct `LimitReader`; require the file wrapper to open and call
only that primitive; require `ToolContext::read_policy_bounded` to call only the file wrapper. Forbid
`metadata()`-based acceptance and direct `File::read_to_end` elsewhere. Disposable fixtures must
fail for a faithful metadata-size check followed by an unbounded read, a file wrapper that duplicates
the read, and a context method that bypasses either layer. Wire the guard to pre-commit
for `config.rs`, `main.rs`, `tools/mod.rs`, all five tool modules, the guard, and its self-test. This
structural proof, not sparse-file metadata alone, discriminates the metadata-only mutation.

- [ ] **Step 5: Run GREEN reader/config tests and guard**

```bash
cargo test --locked -p assay-mcp-server config:: -- --nocapture
cargo test --locked -p assay-mcp-server tools::tests::bounded_policy -- --nocapture
python3 scripts/ci/check-mcp-policy-reader-routing.py
bash scripts/ci/test-check-mcp-policy-reader-routing.sh
```

- [ ] **Step 6: Commit the shared reader**

```bash
git add -- \
  crates/assay-mcp-server/src/config.rs \
  crates/assay-mcp-server/src/main.rs \
  crates/assay-mcp-server/src/tools/mod.rs \
  scripts/ci/check-mcp-policy-reader-routing.py \
  scripts/ci/test-check-mcp-policy-reader-routing.sh \
  .pre-commit-config.yaml
git commit -m "feat(mcp): bound shared policy-file reads"
```

### Task 3: Create one full-policy bytes parser

**Files:**
- Modify: `crates/assay-core/src/mcp/policy/mod.rs`
- Modify: `crates/assay-core/src/mcp/policy/legacy.rs`
- Modify: `crates/assay-core/src/mcp/tests.rs`
- Verify: `crates/assay-core/tests/mcp_policy_warning_contract.rs` (landed by #2387)
- Add: `scripts/ci/check-mcp-policy-parser-delegation.py`
- Add: `scripts/ci/test-check-mcp-policy-parser-delegation.sh`
- Modify: `.pre-commit-config.yaml`

- [ ] **Step 1: Freeze current file-parser behavior before extraction**

For each fixture, pin an independent expected outcome through the existing
`McpPolicy::from_file` API:

```text
V2 tools/schema policy
legacy root allow/deny
V1 constraints and migration
unknown field (accepted with warning)
non-strict V1 input (accepted with the existing deprecation warning)
strict-deprecations rejection
malformed YAML
validation failure (for example invalid referenced rule)
```

Expected outcomes must name the normalized allow/deny/schema result for V2 and legacy fixtures,
the migrated schema produced from V1 constraints, the unknown-field warning captured with a test
tracing subscriber, the separate non-strict V1 deprecation warning, strict-mode rejection,
syntax/validation failure kind, and whether validation ran. Run this table GREEN and commit it as a
behavior freeze; it is not RED evidence.

Re-run the inherited #2387 subprocess warning contract before extraction; do not recreate or weaken
its `OnceLock`, ordering, or once-per-process assertions in this slice.

```bash
cargo test --locked -p assay-core mcp::tests::policy_file_parser_contract -- --nocapture
cargo test --locked -p assay-core --test mcp_policy_warning_contract -- --nocapture
git add -- \
  crates/assay-core/src/mcp/tests.rs
git commit -m "test(mcp): freeze full-policy file parser behavior"
```

- [ ] **Step 2: Add an interface-only stub, then run a behavioral RED**

Add `McpPolicy::from_slice(&[u8])` and required `McpPolicy::from_str(&str)` methods that return one
explicit temporary unsupported error without parsing. This is an interface seam, not a second
parser. Then extend the fixture table to call file, bytes, and string entry points, plus non-UTF-8
bytes for `from_slice`, and assert each independent expected outcome before asserting parity.

```bash
cargo test --locked -p assay-core mcp::tests::policy_parser_contract -- --nocapture
```

Expected: the test compiles and fails assertions because the bytes/string stub returns unsupported.
Record that behavioral failure and commit the interface-only stub plus test before moving parser
behavior:

```bash
git add -- \
  crates/assay-core/src/mcp/policy/mod.rs \
  crates/assay-core/src/mcp/policy/legacy.rs \
  crates/assay-core/src/mcp/tests.rs
git commit -m "test(mcp): expose missing full-policy bytes parser"
```

- [ ] **Step 3: Move the rule, do not copy it**

Move UTF-8 decode, deserialize-with-`serde_ignored`, unknown-field warning, non-strict V1
deprecation warning, strict-deprecation check, legacy normalization, constraint migration, and
validation into one bytes-in function in `legacy.rs`. Public `McpPolicy::from_slice` must delegate
to that legacy function; public `from_str` must delegate to public `from_slice`.

- [ ] **Step 4: Make `from_file` delegate**

The public `McpPolicy::from_file` in `policy/mod.rs` delegates only to `legacy::from_file`.
`legacy::from_file` performs only the file read and `McpPolicy::from_slice(&bytes)`. Neither function
may retain a deserialize/normalize/validate sequence.

- [ ] **Step 5: Add and self-test the structural delegation guard**

Add a source guard that identifies all four exact delegation hops: the public `McpPolicy::from_file`
body in `policy/mod.rs` must call `legacy::from_file`, and `legacy::from_file` must call
`McpPolicy::from_slice`; public `McpPolicy::from_slice` must call the single legacy bytes function;
public `McpPolicy::from_str` must call `McpPolicy::from_slice`. Reject parser/deserializer
construction in all four bodies. The self-test uses
disposable source fixtures and must fail when a faithful duplicate deserializer is inserted at
any hop. Wire the guard into pre-commit for changes to `policy/mod.rs`, `legacy.rs`, the guard,
or its self-test.

- [ ] **Step 6: Run core GREEN and the structural guard**

```bash
cargo test --locked -p assay-core mcp::tests -- --nocapture
cargo test --locked -p assay-core --test mcp_policy_warning_contract -- --nocapture
python3 scripts/ci/check-mcp-policy-parser-delegation.py
bash scripts/ci/test-check-mcp-policy-parser-delegation.sh
```

- [ ] **Step 7: Run semantic mutations and commit parser extraction**

In disposable copies, separately skip the non-strict V1 deprecation warning, strict-deprecation
detection, legacy normalization, constraint migration, unknown-field reporting, and validation.
Each must fail its own expected-outcome assertion even though file, bytes, and string entry points
still agree.

```bash
git add -- \
  crates/assay-core/src/mcp/policy/mod.rs \
  crates/assay-core/src/mcp/policy/legacy.rs \
  crates/assay-core/src/mcp/tests.rs \
  scripts/ci/check-mcp-policy-parser-delegation.py \
  scripts/ci/test-check-mcp-policy-parser-delegation.sh \
  .pre-commit-config.yaml
git commit -m "refactor(mcp): share full-policy bytes parser"
```

### Task 4: Route all five tools through the bound

**Files:**
- Modify: `crates/assay-mcp-server/src/tools/policy_decide.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_sequence.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_coverage.rs`
- Modify: `crates/assay-mcp-server/src/tools/explain_trace.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_args.rs`
- Modify: `crates/assay-mcp-server/tests/policy_ingest_limits.rs`

- [ ] **Step 1: Complete the already-RED real-stdio matrix**

The Task 1 test already proves limit-plus-one fails behaviorally on the unmodified server. Extend it
with an exactly-limit valid policy in each tool's own dialect and assert it reaches the normal
non-limit result. Pad valid YAML with comments to the exact byte boundary; do not pretend one
unpadded fixture can have identical valid semantics in all five dialects. For
`assay_policy_decide` and `assay_check_sequence`, call oversized input twice and assert both remain
limit failures.

Retain the invalid-UTF-8 `assay_check_args` regression control from Task 1. It must remain GREEN with
`result.isError:true`, `allowed:false`, `E_POLICY_PARSE`, and exactly `Policy YAML is invalid`; it is
not claimed as a new RED in this slice.

- [ ] **Step 2: Re-run the behavioral RED and commit the completed matrix**

```bash
cargo test --locked -p assay-mcp-server --test policy_ingest_limits -- --nocapture
```

Expected: limit-plus-one assertions still fail because no tool is routed through the reader;
exact-limit and invalid-UTF-8 controls stay green. Record the discriminating failures and commit
only `policy_ingest_limits.rs` before migrating tool reads.

- [ ] **Step 3: Replace every direct read**

Replace the four `tokio::fs::read` blocks with `ctx.read_policy_bounded(policy_rel_path).await`.
Keep hashing, parsing, cache lookup/insertion, and evaluation after successful bytes exactly where
they are.

- [ ] **Step 4: Remove `check_args`' file bypass**

Have `check_args` call the same context reader and pass returned bytes to `McpPolicy::from_slice`.
It must not call `McpPolicy::from_file` or open the path itself.

- [ ] **Step 5: Run GREEN, including the routing guard**

```bash
cargo test --locked -p assay-mcp-server --test policy_ingest_limits -- --nocapture
cargo test --locked -p assay-mcp-server --test policy_decide_blocklist -- --nocapture
cargo test --locked -p assay-mcp-server --test stdio_edge_cases -- --nocapture
python3 scripts/ci/check-mcp-policy-reader-routing.py
bash scripts/ci/test-check-mcp-policy-reader-routing.sh
```

- [ ] **Step 6: Run discriminating mutations**

In disposable copies, separately replace one tool callsite with `tokio::fs::read`; make
`check_args` call `from_file`; map invalid UTF-8 to `E_POLICY_READ` or `E_INTERNAL`; replace the
reader with a metadata-only check; and change the inclusive boundary to reject exactly-limit. The
metadata-only mutation must fail the structural reader guard even when file metadata matches the
fixture size. Each mutation must fail a named matrix/classification/structural/boundary assertion.

- [ ] **Step 7: Commit all five migrations**

```bash
git add -- \
  crates/assay-mcp-server/src/tools/policy_decide.rs \
  crates/assay-mcp-server/src/tools/check_sequence.rs \
  crates/assay-mcp-server/src/tools/check_coverage.rs \
  crates/assay-mcp-server/src/tools/explain_trace.rs \
  crates/assay-mcp-server/src/tools/check_args.rs \
  crates/assay-mcp-server/tests/policy_ingest_limits.rs
git commit -m "fix(mcp): route policy tools through bounded ingest"
```

### Task 5: Document the operator-visible ceiling

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `docs/reference/cli/mcp-server.md`

- [ ] **Step 1: Record behavior and migration**

Document the inclusive `1_000_000`-byte default, `ASSAY_MCP_MAX_POLICY_BYTES`, the fixed
`E_LIMIT_EXCEEDED` result, and that operators with previously accepted larger local policy files
must either reduce the file or set an explicit bounded override. State that the limit applies before
parse/cache and is independent from the JSON-RPC message ceiling. Do not claim nesting-depth or
remote-input protection.

- [ ] **Step 2: Verify docs and commit**

```bash
pre-commit run --files CHANGELOG.md docs/reference/cli/mcp-server.md
git add -- CHANGELOG.md docs/reference/cli/mcp-server.md
git commit -m "docs(mcp): document policy ingest ceiling"
```

### Task 6: Verify and publish #2389

- [ ] **Step 1: Prove no server-tool bypass remains**

```bash
rg -n 'tokio::fs::read|std::fs::read(_to_string)?|McpPolicy::from_file' \
  crates/assay-mcp-server/src/tools
```

Expected: no policy-reader bypass in the five tool modules. Test fixtures may still write files.

- [ ] **Step 2: Run affected suites and static checks**

```bash
cargo test --locked -p assay-core mcp::tests -- --nocapture
cargo test --locked -p assay-core --test mcp_policy_warning_contract -- --nocapture
cargo test --locked -p assay-mcp-server --test policy_ingest_limits -- --nocapture
cargo test --locked -p assay-mcp-server
cargo fmt --all -- --check
cargo clippy -p assay-core -p assay-mcp-server --all-targets -- -D warnings
git diff --check
git status --short
```

- [ ] **Step 3: Inspect scope and non-claims**

```bash
git diff --stat origin/main...HEAD
git diff --name-only origin/main...HEAD
```

Confirm proxy startup, manifest/trust readers, CLI readers, parse depth/aliases, remote transport,
and verdict dialects are untouched.

- [ ] **Step 4: Push, open the PR, and record #2388**

Push `codex/2389-policy-bounded-ingest`, open against current `main`, and record branch, PR, exact
head, tests, mutation evidence, non-claims, and findings in the programme ledger.

- [ ] **Step 5: Obtain exact-head quorum**

Use one fresh non-building reviewer who authored neither the governing spec/plan nor the code. Any
push invalidates the review. Merge only after required checks, exact-head review, and exact-head
delegated proof when triggered.
