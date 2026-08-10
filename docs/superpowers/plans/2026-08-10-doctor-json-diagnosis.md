# Doctor JSON Diagnosis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `assay doctor` publish the registered top-level `reason_code` and `next_step` on an unloadable explicit config, and give that failure the frozen configuration class instead of the test-failure class.

**Architecture:** Route the whole decision through one private helper that calls `RunOutcome::from_reason(ReasonCode::ECfgParse, ..)`. The registry then decides the reason identity, the recovery step and the exit class once, so the JSON report cannot disagree with the text path, and neither can disagree with `assay run` on the same file. The JSON report keeps `assay.doctor_report.v0`; it gains two fields on the failure path and is untouched on success.

**Tech Stack:** Rust, serde_json, the generated agent-golden-path contract, shared bounded-process test support.

## Global Constraints

- Implement on `cursor/2160-doctor-json-contract` in an isolated worktree with its own `CARGO_TARGET_DIR`.
- Confirm RED against the real binary before production edits, and implement the smallest passing change.
- Do not change the human text renderer. The text `println!` lines stay byte-identical; only the exit class moves, because an exit code that depends on `--format` is worse than one that is uniformly wrong.
- Do not change the doctor report schema id and do not turn the report into `assay.run_summary.v1`; document identity is #2167's subject.
- Regenerate the golden-path contract with `scripts/docs/generate-agent-golden-path.py` rather than editing `docs/generated/` by hand.
- Stage only explicit paths touched by this slice.

## Non-Goals

- Splitting the `E_CFG_PARSE` identity so an absent config reads differently from a malformed one. The message already says "failed to read" while the code says parse; that conflation predates this slice and is filed separately.
- The `--fix`-with-JSON and `--yes`-without-`--fix` usage rejections, which still return the test-failure class.

---

### Task 1: Move the contract, then satisfy it

**Files:**
- Modify: `scripts/docs/generate-agent-golden-path.py`
- Modify: `crates/assay-cli/tests/agent_golden_path_contract.rs`
- Modify: `crates/assay-cli/src/cli/commands/doctor/implementation.rs`
- Modify: `crates/assay-cli/tests/config_recovery_argv_contract.rs`

**Step 1: Declare the repaired outcome in the generator**

- [ ] Change the `invalid-config` outcome to exit `2`, `reason_code="E_CFG_PARSE"`, and a `next_step` that carries the same `<config>` placeholder the argv uses.
- [ ] Drop `gap_issue=2160` and rewrite the step summaries to describe the published envelope.
- [ ] Regenerate; confirm the JSON, the guide, both skills and the plugin copies move together.

**Step 2: Assert the diagnosis in the contract test**

- [ ] Replace the `assert_no_diagnosis` / `assert_gap(.., 2160)` pair with the assertions the policy-validation outcome already uses: reason equality against the contract, and `next_step` equality after substituting the real path for `<config>`.
- [ ] Assert `gap_issue` is null, so a regenerated contract that re-opens the gap fails the test.
- [ ] Rename the test away from "current surface" and update the name in the coverage registry.
- [ ] Run it. **Expected RED:** the exit assertion fails with `left: 1  right: 2`.

**Step 3: Implement the smallest passing change**

- [ ] Add `config_failure(path, message) -> RunOutcome` and call it from both channels.
- [ ] In the JSON branch insert `reason_code` and `next_step`, and read `config_error.code` from `outcome.reason_code` so the literal `"E_CFG_PARSE"` disappears.
- [ ] Return `outcome.exit_code` from the JSON branch and from the non-`--fix` text branch.

**Step 4: Reconcile the recovery contract**

- [ ] `config_recovery_argv_contract.rs` executed the published recovery and pinned it to exit `1` while the failure it recovers from was exit `2`. Assert the two agree rather than pinning a literal, so the invariant survives a future class change.

**Verification:**

- [ ] `cargo test -p assay-cli --test agent_golden_path_contract --test config_recovery_argv_contract`
- [ ] Three mutation controls, each expected to fail at a different assertion: exit reverted to `1`, a wrong reason identity, and an omitted `next_step`.
- [ ] `python3 scripts/ci/test-agent-golden-path-skill.py`
- [ ] `cargo test -p assay-cli`, `cargo fmt --all -- --check`, `cargo clippy -p assay-cli --all-targets -- -D warnings`
- [ ] Drive the built binary: both channels exit `2`, `assay run` on the same file exits `2`, the published recovery exits `2`, and the success report gains no fields.
