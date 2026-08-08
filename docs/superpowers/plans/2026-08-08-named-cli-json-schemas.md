# Named CLI JSON Schemas Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each JSON document observed on the validate/run golden path a stable, named schema identity without breaking the public `Summary` Rust type.

**Architecture:** Existing validate, run-results, and summary builders remain responsible for their own documents. The summary writer gains one renderer shared by file output and early-failure stdout; the public `Summary` data type remains unchanged.

**Tech Stack:** Rust, serde_json, assertive binary integration tests, Markdown contracts.

## Global Constraints

- Preserve `schema_version: 1`; add identities without wrapping or renaming existing fields.
- Do not add a field to public `assay_core::report::summary::Summary`.
- Define each schema id once and make every output path call its owning renderer.
- Preserve existing summary key order and existing render-safety behavior.
- Use `CARGO_TARGET_DIR=/tmp/assay-target-2159` for all Rust commands.
- Stage only exact paths touched by this issue.
- Obtain Claude Code Desktop read-only review on every final head.

---

### Task 1: Freeze the Three Binary Surfaces

**Files:**
- Create: `crates/assay-cli/tests/named_json_envelopes.rs`

**Interfaces:**
- Consumes: `CARGO_BIN_EXE_assay`, `init --preset dev --hello-trace`, validate/run JSON output, and `summary.json`.
- Produces: one failing integration contract over the three schema identities.

- [ ] **Step 1: Write the failing binary test**

Create a temporary initialized project, drive validate success/failure and run
success/failure, parse stdout, and assert:

```rust
assert_eq!(validate_success["schema"], "assay.validate_report.v1");
assert_eq!(validate_failure["schema"], "assay.validate_report.v1");
assert_eq!(run_success["schema"], "assay.run_report.v1");
assert_eq!(run_failure["schema"], "assay.run_summary.v1");
assert_eq!(summary_file["schema"], "assay.run_summary.v1");
```

Require integer `schema_version == 1` on all paths and require the three ids to
be distinct.

- [ ] **Step 2: Run the test and verify RED**

```bash
CARGO_TARGET_DIR=/tmp/assay-target-2159 \
  cargo test -p assay-cli --test named_json_envelopes -- --nocapture
```

Expected: FAIL because every `schema` key is absent and run-success also lacks
`schema_version`.

### Task 2: Add the Three Named Renderers

**Files:**
- Modify: `crates/assay-cli/src/cli/commands/validate.rs`
- Modify: `crates/assay-core/src/report/json.rs`
- Modify: `crates/assay-core/src/report/summary/writer.rs`
- Modify: `crates/assay-core/src/report/summary.rs`
- Modify: `crates/assay-cli/src/cli/commands/reporting.rs`
- Test: `crates/assay-core/src/report/json.rs`
- Test: `crates/assay-core/src/report/summary/writer.rs`

**Interfaces:**
- Produces: `VALIDATE_REPORT_SCHEMA`, `RUN_REPORT_SCHEMA`, `RUN_REPORT_SCHEMA_VERSION`, `SUMMARY_SCHEMA`, and `render_summary_json(&Summary) -> anyhow::Result<String>`.
- Consumes: unchanged `Summary`, existing `render_json(&RunArtifacts)`, and existing validate JSON builder.

- [ ] **Step 1: Add focused core tests and verify RED**

Pin the run-report id/version, summary id, first summary key, legacy/new
deserialization, and render-read-render idempotence. Run:

```bash
CARGO_TARGET_DIR=/tmp/assay-target-2159 \
  cargo test -p assay-core report::json::tests report::summary::writer::tests
```

Expected: compile/test failure because the constants and summary renderer do
not exist.

- [ ] **Step 2: Implement the minimal rendering changes**

Add schema constants beside each renderer. Add `schema` and integer
`schema_version` to the run-results JSON object. Add `schema` to validate's
JSON object. Implement the summary renderer by serializing the unchanged
struct, inserting `schema`, and pretty-printing; call it from `write_summary`
and `reporting.rs`.

- [ ] **Step 3: Run focused tests and verify GREEN**

```bash
CARGO_TARGET_DIR=/tmp/assay-target-2159 \
  cargo test -p assay-core report::json::tests report::summary::writer::tests
CARGO_TARGET_DIR=/tmp/assay-target-2159 \
  cargo test -p assay-cli --test named_json_envelopes -- --nocapture
```

Expected: all focused tests pass.

### Task 3: Update the Normative and CLI Contracts

**Files:**
- Modify: `docs/architecture/SPEC-PR-Gate-Outputs-v1.md`
- Modify: `docs/reference/cli/validate.md`
- Modify: `docs/reference/cli/run.md`
- Modify: `docs/AIcontext/run-output.md`

**Interfaces:**
- Consumes: the three exact schema ids from Task 2.
- Produces: public documentation that distinguishes run success, run early failure, and summary artifacts.

- [ ] **Step 1: Update documentation**

Add `assay.run_summary.v1` to the normative summary table/examples/history;
add validate and run identity sections; name success/failure behavior exactly;
record #2167 and #2168 only as bounded follow-ups where needed.

- [ ] **Step 2: Verify literals and public claims**

```bash
rg -n 'assay\.(validate_report|run_report|run_summary)\.v1' \
  crates/assay-cli/src crates/assay-core/src docs/architecture/SPEC-PR-Gate-Outputs-v1.md \
  docs/reference/cli/validate.md docs/reference/cli/run.md docs/AIcontext/run-output.md
rg -n 'trust score|safe agent|certif|compliance claim' \
  docs/architecture/SPEC-PR-Gate-Outputs-v1.md docs/reference/cli/validate.md \
  docs/reference/cli/run.md docs/AIcontext/run-output.md || true
```

Expected: each id appears only in its constant/tests/docs; no expanded claim.

### Task 4: Verify, Mutate, Review, and Deliver

**Files:**
- Modify: `docs/superpowers/plans/2026-08-08-named-cli-json-schemas.md`

**Interfaces:**
- Consumes: completed code, tests, and docs.
- Produces: a verified exact-head PR that closes #2159 and unblocks #2154.

- [ ] **Step 1: Run affected verification**

```bash
CARGO_TARGET_DIR=/tmp/assay-target-2159 cargo test -p assay-core
CARGO_TARGET_DIR=/tmp/assay-target-2159 cargo test -p assay-cli
CARGO_TARGET_DIR=/tmp/assay-target-2159 \
  cargo clippy -p assay-core -p assay-cli --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 2: Kill the three schema mutations**

On reversible copies, change each schema id in its production renderer and
require the focused owning test to fail for that exact mismatch. Restore the
tree and rerun all focused tests.

- [ ] **Step 3: Commit with exact pathspecs**

Stage only the two plan/spec files and the implementation/test/documentation
paths listed above. Do not use a whole-tree staging command.

- [ ] **Step 4: Obtain exact-head Claude Code Desktop review**

Give Claude Desktop the full commit SHA and `origin/main...<sha>` diff in
read-only mode. Fix every actionable finding and repeat on any new head.

- [ ] **Step 5: Push, open a ready PR, and satisfy quorum**

Push `codex/2159-named-json-schemas`, open a ready PR linked to #2159, request
Copilot, and record the exact head, verification, Claude review, non-goals,
and follow-ups #2167/#2168. Merge only after required checks and exact-head
review quorum are satisfied.
