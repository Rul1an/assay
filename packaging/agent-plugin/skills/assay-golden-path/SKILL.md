---
name: assay-golden-path
description: Drive Assay's install-to-evidence golden path and interpret its stdout and exit codes. Use when an agent must operate or diagnose Assay; do not use it to infer provider execution, external side effects, or a clean result from missing output.
---

# Assay Golden Path

Drive the nine steps below in order. Read stdout and the process exit code
separately; a policy denial can be a successful JSON-RPC exchange rather than
a process failure.

`references/agent-golden-path.json` is the authoritative machine contract bundled with this skill.
Read it when exact argv, fields, or per-outcome metadata are needed.
Resolve `references/` and `assets/` paths relative to this SKILL.md directory.

When a step has no `working_directory`, run it from the invocation cwd.
Protected-action fixtures named by the contract are bundled at `assets/privileged-action-gate`.
A present `working_directory` in the contract resolves through that mapping.
Replace `<python>` with `python3` on Unix-like hosts or `python` on Windows.

Empty stdout in a gap row is an observed limitation, not permission for a caller to infer success from missing evidence.
Do not replace a linked gap with an inferred clean result.

This package uses the Agent Plugins 1.0.0 portable skill layout.
Host discovery and marketplace acceptance require separate evidence.

## Journey

### 1. Install check

Run: `assay version`

Exit: Success `0`.

Stdout: One `MAJOR.MINOR.PATCH` line.

On failure: A missing or unstartable binary is a host spawn failure: no Assay process runs, so Assay produces no stdout or exit code.

### 2. Preflight

Run: `assay doctor --format json --config <config>`

Exit: Success `0`; no config examined `0`; config examined, error-severity diagnostic `2`; absent explicit config `2`; invalid explicit config `2`.

Stdout: Parses as `assay.doctor_report.v0`. Every report carries `config_check.status`, one of `checked`, `skipped` or `failed`. Exit `0` on its own does not mean a config was examined: read `config_check.status` to tell a clean config from no config. A config that was examined and carries an error-severity `data_diagnostics[]` entry exits `2`, the class `decide_exit` gives that diagnostic for `assay validate` and `assay run` too; the text channel returns the same class for the same tree. A config failure remains JSON and carries the top-level `reason_code` and `next_step` alongside `config_error.code`.

On failure: An explicit config that will not load exits `2`. `assay run` gives the same class for the same file. A proven-absent path publishes `E_MISSING_CONFIG` and `assay init`; an unloadable path publishes `E_CFG_PARSE` and the fused doctor argv.

### 3. Starter files

Run: `assay init --preset dev --hello-trace`

Exit: Success `0`; unknown preset `2`; success with `--format json` `0`; unknown preset with `--format json` `2`.

Stdout: Default `text` is human progress; success ends with `Next: assay validate --config=eval.yaml --trace-file=traces/hello.jsonl --format json`, and a failing run writes partial progress text rather than the fatal diagnosis. `--format json` replaces that stream with one `assay.init_report.v0` document naming `reason_code`, `next_step`, and the files created and skipped.

On failure: Under `--format json` a rejected `--preset` publishes `E_INVALID_ARGS` and a `next_step` on stdout. Failures the reason-code registry does not name, such as a filesystem write error, still produce no document: stdout is empty and the diagnosis stays on stderr.

### 4. Policy validation

Run: `assay policy validate --input <policy> --format json`

Exit: Valid `0`; malformed `2`.

Stdout: Both paths parse as `assay.run_summary.v1`; valid has exit `0` and an empty reason, while malformed YAML carries `E_POLICY_PARSE`. Other load or schema failures remain stderr-only until they receive an honest reason code.

On failure: Malformed policy is exit `2` and names the failing policy in a concrete JSON argv next step. Missing files and schema failures are not classified as parse failures.

### 5. Evaluation result

Run: `assay run --config eval.yaml --trace-file traces/hello.jsonl --format json`

Exit: All tests pass `0`; completed run with failed tests `1`.

Stdout: Both completed outcomes parse as `assay.run_report.v1`; failed results carry `status: fail` inside `results`.

On failure: Exit `1` is a completed results report, not an early-failure diagnosis; `reason_code` and `next_step` are absent by design.

### 6. Protected action

Working directory: `assets/privileged-action-gate`

Run: `assay-mcp-server proxy-enforce --upstream-command <python> --upstream-arg -u --upstream-arg mock_github_mcp.py --enforce-policy policies/no-allowance.yaml --declared-mcp-manifest baseline-approved.json`

Exit: Policy-denied call after stdin closes `0`; startup input failure `1`.

Stdout: The denied `tools/call` response pins `error.code: -32042`, `error.data.origin: assay-proxy`, and `error.data.reason: no_declared_allowance`.

On failure: Policy denial is not a process failure. A missing enforcement policy fails startup with empty stdout and no stable reason/next-step object: [gap #2163](https://github.com/Rul1an/assay/issues/2163).

### 7. Evidence inspection

Run: `assay evidence show --format json -- <bundle>`

Exit: Valid `0`; Valid with verification disabled `0`; integrity failure `2`; unreadable bundle `2`; format-contract failure `2`.

Stdout: Success parses as an object containing `manifest`, `events`, and `verify_mode`; the registered values are `enabled` and `disabled`, with `--no-verify` producing `disabled`. A recorded-value mismatch parses as `assay.run_summary.v1` with `E_EVIDENCE_INTEGRITY`; an unreadable path uses `E_EVIDENCE_UNREADABLE`.

On failure: Only the four verifier codes that establish a recorded-value mismatch map to `E_EVIDENCE_INTEGRITY`; I/O, gzip, and tar failures use `E_EVIDENCE_UNREADABLE`. Format-contract failures still exit `2` with empty stdout: [gap #2412](https://github.com/Rul1an/assay/issues/2412).

### 8. Offline profile verification

Run: `assay evidence verify-privileged-mcp-action <bundle> --format json`

Exit: Valid `0`; integrity or profile failure `2`.

Stdout: Both paths parse as `assay.privileged_mcp_action.verify.report.v0`. Success has `bundle_integrity: pass` and `verdict: valid` and omits diagnosis. Tamper has `bundle_integrity: fail`, a bounded finding, no verdict, and `E_EVIDENCE_INTEGRITY`.

On failure: A recorded-value mismatch publishes `E_EVIDENCE_INTEGRITY`. A typed `Contract*` defect publishes `E_EVIDENCE_CONTRACT`. Untyped I/O and archive-read failures publish `E_EVIDENCE_UNREADABLE`. A stage-1 pass whose profile verdict is invalid publishes `E_EVIDENCE_PROFILE_INVALID`. Ceiling refusals and `SecurityPathTraversal` publish `E_EVIDENCE_LIMIT_EXCEEDED` and `E_EVIDENCE_PATH_REJECTED`. Success omits both diagnostic fields. `findings[].detail` may retain the caller argv path; `next_step` is path-free.

### 9. SARIF projection

Run: `assay-mcp-server enforcement-sarif --input - --output -`

Exit: Valid input `0`; malformed non-empty NDJSON `1`.

Stdout: Valid input produces SARIF `2.1.0`. A malformed non-empty line produces no SARIF document.

On failure: Malformed NDJSON fails before projection and names the invalid input line on stderr. Blank lines remain accepted.

## Non-claims

- The contract records current behavior; gap rows are not clean results.
- Schema identity conventions outside this narrow contract remain owned by issue #2167.
- A passing evidence integrity check does not prove an external side effect.
- An explicit config whose read returns NotFound is E_MISSING_CONFIG; PermissionDenied, IsADirectory, Other, and YAML failures stay E_CFG_PARSE. That class is taken from the config-read I/O kind, not from a second exists() probe. Windows EACCES kind parity is not claimed, and the permission fixture is skipped as root.
- Read config_check.status before reading data_diagnostics: only the value checked means a config was read, and on skipped the absent data_diagnostics records an unchecked config rather than a clean one.
