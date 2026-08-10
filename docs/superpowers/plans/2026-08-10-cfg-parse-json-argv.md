# E_CFG_PARSE JSON Argv Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish `E_CFG_PARSE` recovery as argument-safe JSON argv and prove that a hostile config path remains one process argument.

**Architecture:** Keep `RunOutcome.next_step` and its schema unchanged. Extract one private formatter in `exit_codes.rs` so the two dynamic executable recoveries, `E_CFG_PARSE` and `E_POLICY_PARSE`, cannot drift in representation; leave dynamic non-command prose as prose and static commands unchanged because they contain no caller-controlled value.

**Tech Stack:** Rust, serde_json, std::process::Command, shared bounded-process test support, Markdown contract documentation.

## Global Constraints

- Implement on `codex/2200-cfg-argv` in the isolated worktree with its own `CARGO_TARGET_DIR`.
- Confirm RED before production edits and implement the smallest passing change.
- Preserve the existing summary schema and `next_step` field; add no envelope field.
- Execute parsed recovery argv directly with `Command::args`; never route it through a shell.
- Treat paths containing whitespace and shell metacharacters as one exact argv element.
- Do not change generated golden-path outputs: their doctor row intentionally records the #2160 typed-output gap.
- Stage only explicit paths touched by this slice.

---

### Task 1: Pin and implement the recovery representation

**Files:**
- Modify: `crates/assay-cli/src/exit_codes.rs`
- Modify: `crates/assay-cli/src/cli/commands/pipeline_error.rs`
- Create: `crates/assay-cli/tests/config_recovery_argv_contract.rs`
- Modify: `crates/assay-cli/Cargo.toml`

**Interfaces:**
- Consumes: `ReasonCode::next_step(context: Option<&str>) -> String` and the existing `Run argv: <JSON array>` contract.
- Produces: private `format_recovery_argv(args: &[&str]) -> String`; `E_CFG_PARSE` argv `['assay', 'doctor', '--config', path]` and unchanged `E_POLICY_PARSE` argv semantics.

- [x] **Step 1: Write the failing unit and binary contract assertions**

In `exit_codes.rs`, replace the weak config-path containment assertion with parsing and exact argument comparison:

```rust
let config_path = "cfg file;$(touch should-not-exist).yaml";
let config_next_step = ReasonCode::ECfgParse.next_step(Some(config_path));
assert_eq!(
    config_next_step,
    r#"Run argv: ["assay","doctor","--config","cfg file;$(touch should-not-exist).yaml"]"#,
);
let config_argv: Vec<String> = serde_json::from_str(
    config_next_step
        .strip_prefix("Run argv: ")
        .expect("config recovery must publish JSON argv"),
)
.expect("config recovery argv must parse");
assert_eq!(config_argv, ["assay", "doctor", "--config", config_path]);
```

In `config_recovery_argv_contract.rs`, use the shared bounded-process helper, create malformed YAML at the same hostile filename, parse the emitted `next_step`, assert exact argv, replace only argv element zero with `CARGO_BIN_EXE_assay`, pass `argv[1..]` directly to `Command::args`, and assert the doctor path is reached without Clap usage or creation of the shell sentinel.

- [x] **Step 2: Run the focused tests to verify RED**

Run:

```bash
CARGO_TARGET_DIR="$PWD/target" cargo test -p assay-cli --bin assay exit_codes::tests::test_reason_code_next_step
CARGO_TARGET_DIR="$PWD/target" cargo test -p assay-cli --test config_recovery_argv_contract
```

Expected: the unit test fails because the current string begins `Run:`, and the binary contract fails while parsing the same legacy shell prose.

- [x] **Step 3: Implement one formatter and use it in both dynamic command arms**

Add the private helper:

```rust
fn format_recovery_argv(args: &[&str]) -> String {
    format!("Run argv: {}", serde_json::json!(args))
}
```

Call it from `E_CFG_PARSE` and `E_POLICY_PARSE`. Add a doc comment to `next_step` recording the completed audit:

```rust
/// Executable recovery with caller-controlled values is JSON argv. Dynamic
/// trace-path guidance is prose rather than a command; remaining commands are
/// static strings and therefore carry no caller-controlled argument boundary.
```

Update pipeline parity expectations to parse and compare the JSON argv instead of pinning the retired shell string.

- [x] **Step 4: Run focused tests to verify GREEN**

Run:

```bash
CARGO_TARGET_DIR="$PWD/target" cargo test -p assay-cli --bin assay exit_codes::tests::test_reason_code_next_step
CARGO_TARGET_DIR="$PWD/target" cargo test -p assay-cli --bin assay pipeline_error::tests
CARGO_TARGET_DIR="$PWD/target" cargo test -p assay-cli --test config_recovery_argv_contract
```

Expected: PASS. The end-to-end recovery process reaches `doctor`, emits no Clap usage, and creates no shell sentinel.

### Task 2: Align public recovery references

**Files:**
- Modify: `docs/AIcontext/entry-points.md`
- Modify: `docs/AIcontext/quick-reference.md`
- Modify: `docs/AIcontext/decision-trees.md`
- Modify: `docs/architecture/SPEC-PR-Gate-Outputs-v1.md`
- Review without editing: `scripts/docs/generate-agent-golden-path.py`

**Interfaces:**
- Consumes: the exact `Run argv: ["assay","doctor","--config","<file>"]` runtime representation.
- Produces: public AI-context examples that teach callers to parse JSON and invoke argv directly.

- [x] **Step 1: Replace E_CFG_PARSE shell examples in hand-written references**

Use the exact example:

```text
Run argv: ["assay","doctor","--config","<file>"]
```

Add one concise note that consumers parse the array and pass elements directly to a process API, not a shell.

- [x] **Step 2: Record generated/spec non-changes**

Confirm the golden-path generator only names `E_CFG_PARSE` because #2160 still owns doctor machine output. Align the hand-written PR-gate spec with the existing dynamic-command rule and replace its stale `E_TRACE_NOT_FOUND` shell-command example with the actual prose recovery; do not hand-edit generated outputs or widen this slice.

- [x] **Step 3: Verify public strings and docs drift**

Run:

```bash
rg -n 'E_CFG_PARSE|assay doctor --config|Run argv' docs/AIcontext scripts/docs/generate-agent-golden-path.py docs/architecture/SPEC-PR-Gate-Outputs-v1.md
bash scripts/ci/check-docs-generated-drift.sh
git diff --check
```

Expected: no hand-written E_CFG_PARSE reference advertises a shell-interpolated path; generated outputs are unchanged and the drift check passes.

### Task 3: Verify, mutate, and publish the slice

**Files:**
- Verify all files changed by Tasks 1-2.

**Interfaces:**
- Consumes: final working tree for #2200.
- Produces: exact-SHA verification and review packet for PR review.

- [x] **Step 1: Run affected and repository gates**

```bash
CARGO_TARGET_DIR="$PWD/target" cargo test -p assay-cli
CARGO_TARGET_DIR="$PWD/target" cargo clippy -p assay-cli --all-targets -- -D warnings
CARGO_TARGET_DIR="$PWD/target" cargo package -p assay-cli --allow-dirty --no-verify
cargo fmt --all -- --check
git diff --check
bash scripts/ci/check-docs-generated-drift.sh
```

- [x] **Step 2: Run workflow and mutation probes**

Run the real binary contract with the hostile path. Then temporarily restore the old `E_CFG_PARSE` arm and confirm both the unit and end-to-end tests fail; restore the implementation and rerun both tests green. This proves the tests are not decorative.

- [ ] **Step 3: Commit with explicit pathspec, push, and open the PR**

Stage only the plan, source, test, package metadata, three AI-context files, and the PR-gate spec. Record the exact committed SHA with verification provenance. Request one independent reviewer plus CodeRabbit or Copilot; use two independent agents only if the bot is unavailable under `AGENTS.md`.
