# Agent Golden Path Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish and pin the current eight-step #1975 journey for callers that only consume stdout and exit codes.

**Architecture:** One Markdown table is the public contract. Two package-local Rust integration tests drive their own built binaries, assert stable stdout semantics for success and deliberate failure, and verify the corresponding table rows and measured-gap links.

**Tech Stack:** Rust integration tests, `std::process`, `serde_json`, temporary directories, Markdown.

## Global Constraints

- Do not add or change commands, flags, output formats, or production behavior.
- Drive `CARGO_BIN_EXE_assay` and `CARGO_BIN_EXE_assay-mcp-server`; do not use a binary found on `PATH`.
- Consume stdout and exit status for contract decisions; stderr may appear only in assertion diagnostics.
- Pin schemas, registered codes, JSON-RPC fields, and gap issue ids; do not pin timestamps, host paths, platform values, ANSI output, or free-form prose.
- Preserve the distinction between policy denial, process failure, observed evidence, and verified claims.
- Use `CARGO_TARGET_DIR=/tmp/assay-target-2154` for all Rust verification.

---

### Task 1: CLI Journey Contract

**Files:**
- Create: `crates/assay-cli/tests/agent_golden_path_contract.rs`
- Create: `docs/guides/agent-golden-path.md`
- Modify: `docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md`

**Interfaces:**
- Consumes: `CARGO_BIN_EXE_assay`, committed privileged-action corpus vectors, and temporary starter-policy files.
- Produces: tested rows for install verification, preflight, init, policy validation, evidence inspection, and offline profile verification.

- [x] **Step 1: Write the failing CLI integration test**

Add six tests that run the binary and assert:

```text
version success: exit 0, non-empty semver stdout
doctor failure: exit 1, assay.doctor_report.v0, config_error.code=E_CFG_PARSE
init failure: exit 2, stdout does not contain the fatal diagnosis, gap #2161 documented
policy failure: exit 2, stdout empty, gap #2162 documented
show tamper failure: exit 2, stdout empty, gap #2164 documented
profile verify tamper: exit 2, assay.privileged_mcp_action.verify.report.v0,
                       bundle_integrity=fail, gap #2165 documented
```

The same tests drive successful init, policy validation, show, and profile
verification so each row has both sides where reachable.

- [x] **Step 2: Run the CLI test and verify RED**

```bash
CARGO_TARGET_DIR=/tmp/assay-target-2154 \
  cargo test -p assay-cli --test agent_golden_path_contract -- --nocapture
```

Expected: FAIL because `docs/guides/agent-golden-path.md` does not exist; the
binary-driving setup must otherwise compile.

- [x] **Step 3: Add the minimal CLI rows to the guide**

Write one table row per journey step, with exact invocations and only stable
stdout semantics. Link #2160, #2161, #2162, #2164, and #2165 in the relevant
failure cells.

- [x] **Step 4: Rerun the CLI test and verify GREEN**

Run the command from Step 2. Expected: all tests pass.

### Task 2: MCP Enforcement and Projection Contract

**Files:**
- Create: `crates/assay-mcp-server/tests/agent_golden_path_contract.rs`
- Modify: `docs/guides/agent-golden-path.md`
- Modify: `docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md`

**Interfaces:**
- Consumes: `CARGO_BIN_EXE_assay-mcp-server`, the committed proxy mock/policy/baseline fixtures, and the enforcement-decision fixture.
- Produces: tested rows for a policy-denied privileged action, enforcing-proxy startup failure, valid SARIF projection, and malformed-input gap behavior.

- [x] **Step 1: Write the failing MCP integration test**

Drive `proxy-enforce` over stdio and assert the policy denial:

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
CARGO_TARGET_DIR=/tmp/assay-target-2154 \
  cargo test -p assay-mcp-server --test agent_golden_path_contract -- --nocapture
```

Expected: FAIL because the MCP rows are absent from the guide.

- [x] **Step 3: Complete the guide table**

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
- Produces: verified commit, exact-head Claude Code review, and PR linked to #2154.

- [x] **Step 1: Run affected verification**

```bash
CARGO_TARGET_DIR=/tmp/assay-target-2154 cargo test -p assay-cli --test agent_golden_path_contract
CARGO_TARGET_DIR=/tmp/assay-target-2154 cargo test -p assay-mcp-server --test agent_golden_path_contract
CARGO_TARGET_DIR=/tmp/assay-target-2154 cargo test -p assay-cli --test conformance_privileged_mcp_action --test json_format_reports_failures_on_stdout
CARGO_TARGET_DIR=/tmp/assay-target-2154 cargo test -p assay-mcp-server
CARGO_TARGET_DIR=/tmp/assay-target-2154 cargo clippy -p assay-cli -p assay-mcp-server --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

- [x] **Step 2: Kill the targeted mutations**

On a disposable copy or reversible working-tree mutation, require these
failures before restoring the exact tree:

1. Change `assay.privileged_mcp_action.verify.report.v0` in the guide.
2. Change `no_declared_allowance` in the guide's proxy row.
3. Remove the #2166 gap link.

After restoration, rerun both focused tests and require green.

- [ ] **Step 3: Commit with an exact pathspec**

```bash
git add -A \
  crates/assay-cli/tests/agent_golden_path_contract.rs \
  crates/assay-mcp-server/tests/agent_golden_path_contract.rs \
  docs/guides/agent-golden-path.md \
  docs/superpowers/specs/2026-08-08-agent-golden-path-contract-design.md \
  docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md
git commit -m "test(cli): pin the agent golden-path contract"
```

- [ ] **Step 4: Obtain Claude Code review on the exact commit**

Run Claude Code in plan/read-only mode against the full SHA and the
`origin/main...<sha>` diff. Fix every actionable finding, then repeat the
review on the new exact head.

- [ ] **Step 5: Push and open the PR**

Push `codex/2154-stdout-exit-contract`, open a ready PR linked to #2154, and
record the branch, exact head, verification, reviews, non-claims, and open gap
issues in #2154.
