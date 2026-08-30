# Agent Golden Path: stdout and Exit Codes

This is the machine-facing description of the same install, protect, inspect,
verify, and project journey tracked in
[#1975](https://github.com/Rul1an/assay/issues/1975). It records what the
current commands actually expose to a caller that reads stdout and the process
exit code. It is not a claim that every row already has the desired shape.

The canonical machine contract is
[`docs/generated/agent-golden-path.json`](../generated/agent-golden-path.json).
The table below is rendered from that document by
`scripts/docs/generate-agent-golden-path.py`; edit the generator rather than
the table. Binary-level tests in `assay-cli` and `assay-mcp-server` load the
JSON and drive every recorded outcome through `CARGO_BIN_EXE_*`. Paths in
angle brackets are replaced with temporary files or committed fixtures.

<!-- agent-golden-path-release:start -->
## Release-pinned start

This source tree declares Assay `5.5.0` (`v5.5.0`).
This journey is pinned to Assay `5.4.0` ([`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0)).
Install the CLI from a verified channel, then require `assay version` to print `5.4.0` before using the table below. Behavior merged after that tag is `Unreleased` and is not part of this release claim.

Upgrade by installing a newer explicit release and re-running all nine steps.
Roll back by reinstalling `v5.4.0` from the GitHub release assets and re-running the same journey.
<!-- agent-golden-path-release:end -->

<!-- agent-golden-path-discovery:start -->
## Project Skill Discovery

The generated `assay-golden-path` project skill is available at both:

- `.agents/skills/assay-golden-path/SKILL.md`
- `.claude/skills/assay-golden-path/SKILL.md`

When a step has no `working_directory`, run it from the invocation cwd.
A present `working_directory` is a POSIX path relative to the source repository.
Replace `<python>` with `python3` on Unix-like hosts or `python` on Windows.

Source: [Cursor's skill documentation](https://cursor.com/docs/skills), accessed on `2026-08-09`.
The documentation describes `.agents/skills/` as a project-level location and `.claude/skills/` as a compatibility location. This repository does not exercise Cursor runtime discovery.
<!-- agent-golden-path-discovery:end -->

<!-- agent-golden-path-table:start -->
| step | working directory | command | exit code | stdout | on failure |
|---|---|---|---|---|---|
| 1. Install check | `invocation cwd` | `assay version` | Success `0`. | One `MAJOR.MINOR.PATCH` line. | A missing or unstartable binary is a host spawn failure: no Assay process runs, so Assay produces no stdout or exit code. |
| 2. Preflight | `invocation cwd` | `assay doctor --format json --config <config>` | Success `0`; no config examined `0`; config examined, error-severity diagnostic `2`; absent explicit config `2`; invalid explicit config `2`. | Parses as `assay.doctor_report.v0`. Every report carries `config_check.status`, one of `checked`, `skipped` or `failed`. Exit `0` on its own does not mean a config was examined: read `config_check.status` to tell a clean config from no config. A config that was examined and carries an error-severity `data_diagnostics[]` entry exits `2`, the class `decide_exit` gives that diagnostic for `assay validate` and `assay run` too; the text channel returns the same class for the same tree. A config failure remains JSON and carries the top-level `reason_code` and `next_step` alongside `config_error.code`. | An explicit config that will not load exits `2`. `assay run` gives the same class for the same file. A proven-absent path publishes `E_MISSING_CONFIG` and `assay init`; an unloadable path publishes `E_CFG_PARSE` and the fused doctor argv. |
| 3. Starter files | `invocation cwd` | `assay init --preset dev --hello-trace` | Success `0`; unknown preset `2`; success with `--format json` `0`; unknown preset with `--format json` `2`. | Default `text` is human progress; success ends with `Next: assay validate --config=eval.yaml --trace-file=traces/hello.jsonl --format json`, and a failing run writes partial progress text rather than the fatal diagnosis. `--format json` replaces that stream with one `assay.init_report.v0` document naming `reason_code`, `next_step`, and the files created and skipped. | Under `--format json` a rejected `--preset` publishes `E_INVALID_ARGS` and a `next_step` on stdout. Failures the reason-code registry does not name, such as a filesystem write error, still produce no document: stdout is empty and the diagnosis stays on stderr. |
| 4. Policy validation | `invocation cwd` | `assay policy validate --input <policy> --format json` | Valid `0`; malformed `2`. | Both paths parse as `assay.run_summary.v1`; valid has exit `0` and an empty reason, while malformed YAML carries `E_POLICY_PARSE`. Other load or schema failures remain stderr-only until they receive an honest reason code. | Malformed policy is exit `2` and names the failing policy in a concrete JSON argv next step. Missing files and schema failures are not classified as parse failures. |
| 5. Evaluation result | `invocation cwd` | `assay run --config eval.yaml --trace-file traces/hello.jsonl --format json` | All tests pass `0`; completed run with failed tests `1`. | Both completed outcomes parse as `assay.run_report.v1`; failed results carry `status: fail` inside `results`. | Exit `1` is a completed results report, not an early-failure diagnosis; `reason_code` and `next_step` are absent by design. |
| 6. Protected action | `examples/privileged-action-gate` | `assay-mcp-server proxy-enforce --upstream-command <python> --upstream-arg -u --upstream-arg mock_github_mcp.py --enforce-policy policies/no-allowance.yaml --declared-mcp-manifest baseline-approved.json` | Policy-denied call after stdin closes `0`; startup input failure `1`. | The denied `tools/call` response pins `error.code: -31999`, `error.data.origin: assay-proxy`, and `error.data.reason: no_declared_allowance`. | Policy denial is not a process failure. A missing enforcement policy fails startup with empty stdout and one JSON `startup_failure` event on stderr, including `reason_code: proxy_enforce_policy_invalid` and an actionable `next_step`. |
| 7. Evidence inspection | `invocation cwd` | `assay evidence show --format json -- <bundle>` | Valid `0`; Valid with verification disabled `0`; integrity failure `2`; unreadable bundle `2`; format-contract failure `2`. | Success parses as an object containing `manifest`, `events`, and `verify_mode`; the registered values are `enabled` and `disabled`, with `--no-verify` producing `disabled`. A recorded-value mismatch parses as `assay.run_summary.v1` with `E_EVIDENCE_INTEGRITY`; an unreadable path uses `E_EVIDENCE_UNREADABLE`; a typed `Contract*` defect uses `E_EVIDENCE_CONTRACT`. | Only the four verifier codes that establish a recorded-value mismatch map to `E_EVIDENCE_INTEGRITY`; I/O, gzip, and tar failures use `E_EVIDENCE_UNREADABLE`. Typed `Contract*` failures encountered while opening the bundle publish `E_EVIDENCE_CONTRACT` with a bounded prose `next_step`. Event-line deserialization plus LIMIT/PATH and PROFILE, where they apply, remain exit `2` with empty stdout until typed on this command. |
| 8. Offline profile verification | `invocation cwd` | `assay evidence verify-privileged-mcp-action <bundle> --format json` | Valid `0`; integrity or profile failure `2`. | Both paths parse as `assay.privileged_mcp_action.verify.report.v0`. Success has `bundle_integrity: pass` and `verdict: valid` and omits diagnosis. Tamper has `bundle_integrity: fail`, a bounded finding, no verdict, and `E_EVIDENCE_INTEGRITY`. | A recorded-value mismatch publishes `E_EVIDENCE_INTEGRITY`. A typed `Contract*` defect publishes `E_EVIDENCE_CONTRACT`. Untyped I/O and archive-read failures publish `E_EVIDENCE_UNREADABLE`. A stage-1 pass whose profile verdict is invalid publishes `E_EVIDENCE_PROFILE_INVALID`. Ceiling refusals and `SecurityPathTraversal` publish `E_EVIDENCE_LIMIT_EXCEEDED` and `E_EVIDENCE_PATH_REJECTED`. Success omits both diagnostic fields. `findings[].detail` may retain the caller argv path. Unreadable `next_step` is shell-free caller-argv (concrete JSON `Run argv` with `--` and the caller path), not a shell string. Other owned codes stay prose. |
| 9. SARIF projection | `invocation cwd` | `assay-mcp-server enforcement-sarif --input - --output -` | Valid input `0`; malformed non-empty NDJSON `1`. | Valid input produces SARIF `2.1.0`. A malformed non-empty line produces no SARIF document. | Malformed NDJSON fails before projection and names the invalid input line on stderr. Blank lines remain accepted. |
<!-- agent-golden-path-table:end -->

## Reading the Contract

- An exit code is a process result. A policy denial in step 6 is instead a
  successful JSON-RPC exchange whose response contains an error object.
- `bundle_integrity: pass` says the carried bytes recompute under the profile.
  It does not upgrade producer-reported evidence or prove upstream delivery or
  an external side effect.
- Empty stdout in a gap row is an observed limitation, not permission for a
  caller to infer success from missing evidence.
- The linked issues own runtime changes. This table changes only when a driven
  command result changes with them.

## Verification

From the repository root:

```bash
cargo test -p assay-cli --test agent_golden_path_contract
cargo test -p assay-mcp-server --test agent_golden_path_contract
```

These tests run the Cargo-built binaries through `CARGO_BIN_EXE_*`; they do not
read an unrelated Assay executable from `PATH`. The reference upstream remains
the committed Python fixture used by the protected-action scenario, which
requires `python3` on Unix-like hosts or `python` on Windows.
