# MCP Policy Bounded Ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close #2389 by enforcing a configurable inclusive policy-file byte ceiling before any of
the five advertised MCP tools materialises or parses a local policy.

**Architecture:** Add `max_policy_bytes` to `ServerConfig` and one `ToolContext::read_policy_bounded`
entry point. The entry point performs secure path resolution, then uses
`assay_common::limits::LimitReader<std::fs::File>` inside `tokio::task::spawn_blocking`; it maps
typed limit, not-found, and other I/O errors once. Add a bytes-in `McpPolicy` parser and make both
`from_file` and `assay_check_args` call it, so full-policy semantics have one implementation.

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
- Modify `crates/assay-mcp-server/src/tools/mod.rs`: shared async bounded reader and error mapping.
- Modify five tool files under `crates/assay-mcp-server/src/tools/`: remove unbounded reads.
- Modify `crates/assay-core/src/mcp/policy/mod.rs` and `policy/legacy.rs`: one bytes-in parser.
- Modify `crates/assay-core/src/mcp/tests.rs`: file/bytes/string parser parity table.
- Add `crates/assay-mcp-server/tests/policy_ingest_limits.rs`: real-stdio five-tool boundary table.

### Task 1: Freeze configuration and reader boundaries

**Files:**
- Modify: `crates/assay-mcp-server/src/config.rs` (tests only first)
- Modify: `crates/assay-mcp-server/src/tools/mod.rs` (tests only first)

- [ ] **Step 1: Add independent configuration RED tests**

Use one process-scoped mutex and restore every changed variable before releasing it; do not let
parallel tests observe transient process environment. Pin:

```text
default max_policy_bytes == 1_000_000
ASSAY_MCP_MAX_POLICY_BYTES=1234 -> max_policy_bytes == 1234
ASSAY_MCP_MAX_BYTES=4321 changes max_msg_bytes but not max_policy_bytes
invalid ASSAY_MCP_MAX_POLICY_BYTES leaves the default
```

- [ ] **Step 2: Add pure reader RED tests**

Design the blocking primitive so tests can pass any `Read`, then cover:

```text
exactly limit bytes -> complete Vec
limit + 1 bytes -> typed LimitExceeded(SourceBytes)
chunked reader that crosses after multiple reads -> limit error
reader that returns an ordinary I/O error -> read error
```

Use `LimitExceeded::from_io` for classification; never match rendered error text.

- [ ] **Step 3: Add file-level RED tests**

Through `ToolContext::read_policy_bounded`, cover missing file, directory read failure, an
exact-limit regular file, and a sparse limit-plus-one file. Avoid a Unix-mode permission fixture,
which is not portable and may pass under a privileged runner. The sparse test plus the metadata-only
mutation in Task 4 must prove the implementation reads through the ceiling rather than trusting or
allocating from metadata.

- [ ] **Step 4: Run RED**

```bash
cargo test --locked -p assay-mcp-server config:: -- --nocapture
cargo test --locked -p assay-mcp-server tools::tests::bounded_policy -- --nocapture
```

- [ ] **Step 5: Commit only RED tests**

```bash
git add -- \
  crates/assay-mcp-server/src/config.rs \
  crates/assay-mcp-server/src/tools/mod.rs
git commit -m "test(mcp): expose unbounded policy-file ingest"
```

### Task 2: Implement configuration and shared bounded reader

**Files:**
- Modify: `crates/assay-mcp-server/src/config.rs`
- Modify: `crates/assay-mcp-server/src/tools/mod.rs`

- [ ] **Step 1: Add the single configuration value**

Add `pub max_policy_bytes: usize` to `ServerConfig`, default it to `1_000_000`, and parse only
`ASSAY_MCP_MAX_POLICY_BYTES` in `from_env`. Log the value alongside existing limits in `main.rs`
without introducing a second literal.

- [ ] **Step 2: Extract the blocking primitive**

Implement a helper using:

```rust
let file = std::fs::File::open(path)?;
let mut reader = assay_common::limits::LimitReader::new(
    file,
    limit as u64,
    assay_common::limits::LimitKind::SourceBytes,
);
let mut bytes = Vec::new();
std::io::Read::read_to_end(&mut reader, &mut bytes)?;
```

The helper returns bytes or a typed internal read classification. It must not preallocate from
`metadata().len()`.

- [ ] **Step 3: Add the async context entry point**

`ToolContext::read_policy_bounded(user_path)` first calls the existing secure
`resolve_policy_path`, then moves the owned path and limit into `tokio::task::spawn_blocking`.
Centralize mapping:

```text
NotFound -> E_POLICY_NOT_FOUND with the relative request path only
LimitExceeded -> E_LIMIT_EXCEEDED with a fixed value-free message
other I/O or JoinError -> E_POLICY_READ with a fixed value-free message
```

Do not include canonical roots, OS diagnostics, or policy contents.

- [ ] **Step 4: Run GREEN reader/config tests**

```bash
cargo test --locked -p assay-mcp-server config:: -- --nocapture
cargo test --locked -p assay-mcp-server tools::tests::bounded_policy -- --nocapture
```

- [ ] **Step 5: Commit the shared reader**

```bash
git add -- \
  crates/assay-mcp-server/src/config.rs \
  crates/assay-mcp-server/src/main.rs \
  crates/assay-mcp-server/src/tools/mod.rs
git commit -m "feat(mcp): bound shared policy-file reads"
```

### Task 3: Create one full-policy bytes parser

**Files:**
- Modify: `crates/assay-core/src/mcp/policy/mod.rs`
- Modify: `crates/assay-core/src/mcp/policy/legacy.rs`
- Modify: `crates/assay-core/src/mcp/tests.rs`

- [ ] **Step 1: Add a parser parity RED table**

For each fixture, compare the semantic result of the public bytes/string entry point with a temp
file loaded through `McpPolicy::from_file`:

```text
V2 tools/schema policy
legacy root allow/deny
V1 constraints and migration
unknown field (accepted with warning)
strict-deprecations rejection
malformed YAML
non-UTF-8 bytes
validation failure (for example invalid referenced rule)
```

Compare normalized serialized policy for successes and stable success/error class for failures;
do not assert raw parser wording.

- [ ] **Step 2: Move the rule, do not copy it**

Move deserialize-with-`serde_ignored`, unknown-field warning, strict-deprecation check, legacy
normalization, constraint migration, and validation into one bytes-in function in `legacy.rs`.
Expose it from `McpPolicy` as `from_slice(&[u8])` (and `from_str` only if a real caller needs it).

- [ ] **Step 3: Make `from_file` delegate**

`from_file` performs only the file read and `McpPolicy::from_slice(&bytes)`. It must not retain a
second deserialize/normalize/validate sequence.

- [ ] **Step 4: Run core GREEN**

```bash
cargo test --locked -p assay-core mcp::tests -- --nocapture
```

- [ ] **Step 5: Commit parser extraction**

```bash
git add -- \
  crates/assay-core/src/mcp/policy/mod.rs \
  crates/assay-core/src/mcp/policy/legacy.rs \
  crates/assay-core/src/mcp/tests.rs
git commit -m "refactor(mcp): share full-policy bytes parser"
```

### Task 4: Route all five tools through the bound

**Files:**
- Modify: `crates/assay-mcp-server/src/tools/policy_decide.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_sequence.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_coverage.rs`
- Modify: `crates/assay-mcp-server/src/tools/explain_trace.rs`
- Modify: `crates/assay-mcp-server/src/tools/check_args.rs`
- Add: `crates/assay-mcp-server/tests/policy_ingest_limits.rs`

- [ ] **Step 1: Add a real-stdio RED matrix**

Start the server with `ASSAY_MCP_MAX_POLICY_BYTES=<small fixture limit>`. For each advertised tool,
call a limit-plus-one policy and assert `result.isError:true`, `allowed:false`, and
`E_LIMIT_EXCEEDED`. Call an exactly-limit valid policy in that tool's own dialect and assert it
reaches the normal non-limit result. Pad valid YAML with comments to the exact byte boundary; do not
pretend one unpadded fixture can have identical valid semantics in all five dialects.

- [ ] **Step 2: Pin cache-bearing repeat behavior**

For `assay_policy_decide` and `assay_check_sequence`, call the oversized input twice and assert both
remain limit failures. Capture stderr or expose test-only cache counters only if needed; prefer the
observable repeat contract over adding production introspection.

- [ ] **Step 3: Replace every direct read**

Replace the four `tokio::fs::read` blocks with `ctx.read_policy_bounded(policy_rel_path).await`.
Keep hashing, parsing, cache lookup/insertion, and evaluation after successful bytes exactly where
they are.

- [ ] **Step 4: Remove `check_args`' file bypass**

Have `check_args` call the same context reader and pass returned bytes to `McpPolicy::from_slice`.
It must not call `McpPolicy::from_file` or open the path itself.

- [ ] **Step 5: Run GREEN**

```bash
cargo test --locked -p assay-mcp-server --test policy_ingest_limits -- --nocapture
cargo test --locked -p assay-mcp-server --test policy_decide_blocklist -- --nocapture
cargo test --locked -p assay-mcp-server --test stdio_edge_cases -- --nocapture
```

- [ ] **Step 6: Run discriminating mutations**

In disposable copies, separately replace one callsite at a time with `tokio::fs::read`; make
`check_args` call `from_file`; make `from_file` deserialize independently; replace the reader with a
metadata-only check; and change the inclusive boundary to reject exactly-limit. Each mutation must
fail a named matrix/parity/boundary assertion.

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

### Task 5: Verify and publish #2389

- [ ] **Step 1: Prove no server-tool bypass remains**

```bash
rg -n 'tokio::fs::read|std::fs::read(_to_string)?|McpPolicy::from_file' \
  crates/assay-mcp-server/src/tools
```

Expected: no policy-reader bypass in the five tool modules. Test fixtures may still write files.

- [ ] **Step 2: Run affected suites and static checks**

```bash
cargo test --locked -p assay-core mcp::tests -- --nocapture
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
