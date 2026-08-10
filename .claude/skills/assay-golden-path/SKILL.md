---
name: assay-golden-path
description: Drive Assay's install-to-evidence golden path and interpret its stdout and exit codes. Use when an agent must operate or diagnose Assay; do not use it to infer provider execution, external side effects, or a clean result from missing output.
---

# Assay Golden Path

Drive the nine steps below in order. Read stdout and the process exit code
separately; a policy denial can be a successful JSON-RPC exchange rather than
a process failure.

`docs/generated/agent-golden-path.json` is the authoritative machine contract.
Read it when exact argv, fields, or per-outcome metadata are needed. Edit and
run `scripts/docs/generate-agent-golden-path.py` instead of editing this file.

When a step has no `working_directory`, run it from the invocation cwd.
A present `working_directory` is a POSIX path relative to the source repository.
Replace `<python>` with `python3` on Unix-like hosts or `python` on Windows.

Empty stdout in a gap row is an observed limitation, not permission for a caller to infer success from missing evidence.
Do not replace a linked gap with an inferred clean result.

Codex and Claude Code are the project-skill hosts exercised here.
Cursor's skill documentation (https://cursor.com/docs/skills), accessed on 2026-08-09, describes .agents/skills as a project-level location and .claude/skills as a compatibility location. This repository does not exercise Cursor runtime discovery.

## Journey

### 1. Install check

Run: `assay version`

Exit: Success `0`.

Stdout: One `MAJOR.MINOR.PATCH` line.

On failure: A missing or unstartable binary is a host spawn failure: no Assay process runs, so Assay produces no stdout or exit code.

### 2. Preflight

Run: `assay doctor --format json`

Exit: Success `0`; invalid explicit config `1`.

Stdout: Parses as `assay.doctor_report.v0`. A config failure remains JSON and carries `config_error.code: E_CFG_PARSE`.

On failure: `reason_code` and `next_step` are absent, and the exit is outside the frozen config/usage class: [gap #2160](https://github.com/Rul1an/assay/issues/2160).

### 3. Starter files

Run: `assay init --preset dev --hello-trace`

Exit: Success `0`; unknown preset `2`.

Stdout: Human progress text; success ends with `Next: assay validate --config eval.yaml --trace-file traces/hello.jsonl`. A failing run writes partial progress text, not the fatal diagnosis.

On failure: No machine report, `reason_code`, or `next_step` on stdout: [gap #2161](https://github.com/Rul1an/assay/issues/2161).

### 4. Policy validation

Run: `assay policy validate --input <policy>`

Exit: Valid `0`; malformed `2`.

Stdout: Empty on both paths.

On failure: The diagnosis, registered reason, and next step are absent from stdout: [gap #2162](https://github.com/Rul1an/assay/issues/2162).

### 5. Evaluation result

Run: `assay run --config eval.yaml --trace-file traces/hello.jsonl --format json`

Exit: All tests pass `0`; completed run with failed tests `1`.

Stdout: Both completed outcomes parse as `assay.run_report.v1`; failed results carry `status: fail` inside `results`.

On failure: Exit `1` is a completed results report, not an early-failure diagnosis; `reason_code` and `next_step` are absent by design.

### 6. Protected action

Working directory: `examples/privileged-action-gate`

Run: `assay-mcp-server proxy-enforce --upstream-command <python> --upstream-arg -u --upstream-arg mock_github_mcp.py --enforce-policy policies/no-allowance.yaml --declared-mcp-manifest baseline-approved.json`

Exit: Policy-denied call after stdin closes `0`; startup input failure `1`.

Stdout: The denied `tools/call` response pins `error.code: -32042`, `error.data.origin: assay-proxy`, and `error.data.reason: no_declared_allowance`.

On failure: Policy denial is not a process failure. A missing enforcement policy fails startup with empty stdout and no stable reason/next-step object: [gap #2163](https://github.com/Rul1an/assay/issues/2163).

### 7. Evidence inspection

Run: `assay evidence show <bundle> --format json`

Exit: Valid `0`; integrity failure `2`.

Stdout: Success parses as an object containing `manifest` and `events`. Integrity failure produces no stdout.

On failure: No JSON failure report, `reason_code`, or `next_step`: [gap #2164](https://github.com/Rul1an/assay/issues/2164).

### 8. Offline profile verification

Run: `assay evidence verify-privileged-mcp-action <bundle> --format json`

Exit: Valid `0`; integrity or profile failure `2`.

Stdout: Both paths parse as `assay.privileged_mcp_action.verify.report.v0`. Success has `bundle_integrity: pass` and `verdict: valid`; tamper has `bundle_integrity: fail`, a bounded finding, and no verdict.

On failure: The failure report has no registered `reason_code` or actionable `next_step`: [gap #2165](https://github.com/Rul1an/assay/issues/2165).

### 9. SARIF projection

Run: `assay-mcp-server enforcement-sarif --input - --output -`

Exit: Valid input `0`; malformed non-empty NDJSON `1`.

Stdout: Valid input produces SARIF `2.1.0`. A malformed non-empty line produces no SARIF document.

On failure: Malformed NDJSON fails before projection and names the invalid input line on stderr. Blank lines remain accepted.

## Non-claims

- The contract records current behavior; gap rows are not clean results.
- Schema identity conventions outside this narrow contract remain owned by issue #2167.
- A passing evidence integrity check does not prove an external side effect.
