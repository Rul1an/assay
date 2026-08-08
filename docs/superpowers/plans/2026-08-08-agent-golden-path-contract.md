# Agent Golden Path Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish and pin the current nine-step #1975 journey for callers that only consume stdout and exit codes.

**Architecture:** A generated JSON document is the canonical contract. One generator renders its Markdown table, the existing docs drift gate proves both outputs reproduce, and two package-local Rust integration tests drive their own built binaries against the structured outcomes.

**Tech Stack:** Rust integration tests, bounded JSON-RPC test plumbing, `serde_json`, Python standard library generation, temporary directories, JSON, and Markdown.

## Global Constraints

- Do not add or change commands, flags, output formats, or production behavior.
- Drive `CARGO_BIN_EXE_assay` and `CARGO_BIN_EXE_assay-mcp-server`; do not use a binary found on `PATH`.
- Consume stdout and exit status for contract decisions; stderr may appear only in assertion diagnostics.
- Pin schemas, registered codes, JSON-RPC fields, and gap issue ids; do not pin timestamps, host paths, platform values, ANSI output, or free-form prose.
- Preserve the distinction between policy denial, process failure, observed evidence, and verified claims.
- Keep the broader JSON identity convention explicitly deferred to #2167.
- Export one worktree-unique `CARGO_TARGET_DIR` with `mktemp` and reuse it for all
  Rust verification in this worktree.

---

### Task 1: CLI Journey Contract

**Files:**
- Create: `crates/assay-cli/tests/agent_golden_path_contract.rs`
- Create: `docs/generated/agent-golden-path.json`
- Create: `scripts/docs/generate-agent-golden-path.py`
- Create: `docs/guides/agent-golden-path.md`
- Modify: `docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md`

**Interfaces:**
- Consumes: `CARGO_BIN_EXE_assay`, committed privileged-action corpus vectors, and temporary starter-policy files.
- Produces: tested rows for install verification, preflight, init, policy validation, completed run results, evidence inspection, and offline profile verification.

- [x] **Step 1: Write the failing CLI integration test**

Add seven tests that run the binary and assert:

```text
version success: exit 0, non-empty semver stdout
doctor failure: exit 1, assay.doctor_report.v0, config_error.code=E_CFG_PARSE
init failure: exit 2, stdout does not contain the fatal diagnosis, gap #2161 documented
policy failure: exit 2, stdout empty, gap #2162 documented
completed test failure: exit 1, assay.run_report.v1, status=fail, no diagnosis fields
show tamper failure: exit 2, stdout empty, gap #2164 documented
profile verify tamper: exit 2, assay.privileged_mcp_action.verify.report.v0,
                       bundle_integrity=fail, gap #2165 documented
```

The same tests drive successful init, policy validation, show, and profile
verification so each row has both sides where reachable.

- [x] **Step 2: Run the CLI test and verify RED**

```bash
export CARGO_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/assay-target-2154.XXXXXX")"
cargo test -p assay-cli --test agent_golden_path_contract -- --nocapture
```

Expected: FAIL because `docs/generated/agent-golden-path.json` does not exist; the
binary-driving setup must otherwise compile.

- [x] **Step 3: Add the canonical machine outcomes and render the guide**

Write one structured entry per journey step, with outcome-specific `argv` and
only stable stdout semantics. Resolve declared fixture placeholders in tests,
drive the binary with the resulting vector, and derive each rendered command
from the primary outcome. Link #2160, #2161, #2162, #2164, and #2165 in the
relevant outcomes and render the table from the same definitions.

- [x] **Step 4: Rerun the CLI test and verify GREEN**

Run the command from Step 2. Expected: all tests pass.

### Task 2: MCP Enforcement and Projection Contract

**Files:**
- Create: `crates/assay-mcp-server/tests/agent_golden_path_contract.rs`
- Modify: `scripts/ci/check-docs-generated-drift.sh`
- Modify: `scripts/ci/test-check-docs-generated-drift.sh`
- Modify: `docs/guides/agent-golden-path.md`
- Modify: `docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md`

**Interfaces:**
- Consumes: `CARGO_BIN_EXE_assay-mcp-server`, the committed proxy mock/policy/baseline fixtures, and the enforcement-decision fixture.
- Produces: tested rows for a policy-denied privileged action, enforcing-proxy startup failure, valid SARIF projection, and malformed-input gap behavior.

- [x] **Step 1: Write the failing MCP integration test**

Drive `proxy-enforce` over the existing timeout-bounded `jsonrpc_conn::Conn` and assert the policy denial:

```json
{
  "error": {
    "code": -32042,
    "data": {
      "origin": "assay-proxy",
      "reason": "no_declared_allowance"
    }
  }
}
```

Then drive a missing-policy startup failure and SARIF projection with valid and
malformed NDJSON. Require the guide to link #2163 and #2166 for the measured
empty/fail-open outputs.

- [x] **Step 2: Run the MCP test and verify RED**

```bash
cargo test -p assay-mcp-server --test agent_golden_path_contract -- --nocapture
```

Expected: FAIL because the machine contract is absent.

- [x] **Step 3: Complete the machine outcomes and generated table**

Add the enforcing-proxy and SARIF rows. State that policy denial is a JSON-RPC
result with process exit 0, while startup failure and malformed projection are
separate process-level gaps.

- [x] **Step 4: Rerun both focused tests and verify GREEN**

Run both commands from Tasks 1 and 2. Expected: all tests pass.

### Task 3: Verification, Mutation, Review, and Delivery

**Files:**
- Modify: `docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md`

**Interfaces:**
- Consumes: the completed contract and both focused integration tests.
- Produces: verified commit, exact-head review quorum, and PR linked to #2154.

- [x] **Step 1: Run affected verification**

```bash
export CARGO_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/assay-target-2154.XXXXXX")"
cargo test -p assay-cli --test agent_golden_path_contract
cargo test -p assay-mcp-server --test agent_golden_path_contract
cargo test -p assay-cli --test conformance_privileged_mcp_action --test json_format_reports_failures_on_stdout
cargo test -p assay-mcp-server
cargo clippy -p assay-cli -p assay-mcp-server --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

- [x] **Step 2: Kill the targeted mutations**

Run `scripts/ci/test-check-docs-generated-drift.sh`, which uses reversible
working-tree mutations and requires these failures before restoring the exact
tree:

1. Hand-edit an existing generated diagram.
2. Hand-edit `docs/generated/agent-golden-path.json`.
3. Hand-edit the rendered table in `docs/guides/agent-golden-path.md`.
4. Add an outcome without a matching integration-test driver.
5. Add a duplicate integration-test driver for one outcome.

After restoration, rerun both focused tests and require green.

- [x] **Step 3: Commit with an exact pathspec**

```bash
git add -A \
  crates/assay-cli/tests/agent_golden_path_contract.rs \
  crates/assay-mcp-server/tests/agent_golden_path_contract.rs \
  docs/generated/agent-golden-path.json \
  docs/guides/agent-golden-path.md \
  scripts/docs/generate-agent-golden-path.py \
  scripts/ci/check-docs-generated-drift.sh \
  scripts/ci/test-check-docs-generated-drift.sh \
  .pre-commit-config.yaml \
  docs/superpowers/specs/2026-08-08-agent-golden-path-contract-design.md \
  docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md
git commit -m "test(cli): pin the agent golden-path contract"
```

- [ ] **Step 4: Satisfy the exact-head review quorum**

Run Claude Code in plan/read-only mode against the full SHA and the
`origin/main...<sha>` diff, and obtain CodeRabbit or Copilot review on that
same head. Fix or technically disposition every actionable finding, then
repeat both reviews after any push that changes the head. Record the reviewed
SHA and both verdicts; the builder's self-review does not count.

- [ ] **Step 5: Push and open the PR**

Push `codex/2154-stdout-exit-contract`, open a ready PR linked to #2154, and
enable auto-merge only after required checks are green, the exact-head review
quorum is satisfied, and all findings have a disposition. Record the branch,
exact head, verification, reviews, non-claims, and open gap issues in #2154.
