# Policy Validation Machine Contract Implementation Plan

> **For agentic workers:** implement this plan test-first in the isolated
> `codex/2177-error-funnel` worktree.

**Goal:** Close #2162 and prove the first #2177 typed-error funnel slice by giving
`assay policy validate` an explicit, symmetric machine-output contract.

**Architecture:** `policy validate --format json` emits the existing
`assay.run_summary.v1` document on both valid and malformed input. The valid path
renders from the command; the malformed path returns a typed `CliFailure` that the
single `main.rs` error funnel renders. Both paths call the existing
`render_summary_json` function. Default text mode remains stdout-clean and human
diagnostics remain on stderr.

## Constraints

- Do not add a schema, field, reason code, or exit code.
- Use `E_POLICY_PARSE` for malformed policies and derive its exit and next step
  from the existing `ReasonCode` implementation.
- Do not classify untyped `anyhow::Error` values; they retain `fatal:` and exit 2.
- Do not let `run --format json` pass through the new funnel and render twice.
- Keep render safety in #2168 and schema-identity migration in #2167.
- Generate golden-path artifacts through
  `scripts/docs/generate-agent-golden-path.py`; do not hand-edit generated copies.

### Task 1: Freeze the Binary Contract

**Files:**
- Create: `crates/assay-cli/tests/policy_validate_machine_contract.rs`

- [x] Drive default valid and malformed policies and pin stdout/stderr channels.
- [x] Drive JSON valid and malformed policies and pin schema, exit, reason, and
  remediation semantics.
- [x] Prove deterministic stdout, bounded output, and bounded process duration.
- [x] Run the test and record RED before changing production code.

### Task 2: Add the Typed Funnel and Explicit Format

**Files:**
- Create: `crates/assay-cli/src/cli_failure.rs`
- Modify: `crates/assay-cli/src/main.rs`
- Modify: `crates/assay-cli/src/cli/args/mod.rs`
- Modify: `crates/assay-cli/src/cli/args/policy.rs`
- Modify: `crates/assay-cli/src/cli/commands/policy/validate.rs`

- [x] Add `--format text|json`, defaulting to text.
- [x] Capture machine-output intent before `dispatch` consumes `Cli`.
- [x] Return `CliFailure(E_POLICY_PARSE)` from both parse and schema-compile
  failures.
- [x] Render typed failures at the funnel and preserve legacy handling for
  untyped failures.
- [x] Render valid JSON from the same summary renderer without the inaccurate
  `All tests passed` message.

### Task 3: Update the Generated Golden Path

**Files:**
- Modify: `scripts/docs/generate-agent-golden-path.py`
- Generate: `.agents/skills/assay-golden-path/SKILL.md`
- Generate: `.claude/skills/assay-golden-path/SKILL.md`
- Generate: `packaging/claude-plugin/skills/assay-golden-path/SKILL.md`
- Generate: `packaging/claude-plugin/skills/assay-golden-path/references/agent-golden-path.json`
- Generate: `docs/generated/agent-golden-path.json`
- Generate: `docs/guides/agent-golden-path.md`
- Modify: `crates/assay-cli/tests/agent_golden_path_contract.rs`

- [x] Replace the measured #2162 gap with JSON outcomes on both paths.
- [x] Make the golden-path runtime test drive and inspect both documents.
- [x] Run generator drift and mutation-sensitive contract tests.

### Task 4: Verify and Review

- [x] Focused binary tests.
- [x] `cargo test -p assay-cli`.
- [x] `cargo fmt --all -- --check`.
- [x] `cargo clippy -p assay-cli --all-targets -- -D warnings`.
- [x] `git diff --check` and public-string audit.
- [x] Mutate format routing, reason code, renderer call, and recovery command to
  prove the tests bite.
- [x] Run human, workflow, and agent simulations against the built binary.
- [ ] Commit with an exact pathspec, push, and open a PR.
- [ ] Obtain one independent non-building review on the final SHA; merge only
  after required CI is green and all findings are resolved or dispositioned.
