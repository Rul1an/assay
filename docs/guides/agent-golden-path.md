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
| 2. Preflight | `invocation cwd` | `assay doctor --format json` | Success `0`; invalid explicit config `1`. | Parses as `assay.doctor_report.v0`. A config failure remains JSON and carries `config_error.code: E_CFG_PARSE`. | `reason_code` and `next_step` are absent, and the exit is outside the frozen config/usage class: [gap #2160](https://github.com/Rul1an/assay/issues/2160). |
| 3. Starter files | `invocation cwd` | `assay init --preset dev --hello-trace` | Success `0`; unknown preset `2`. | Human progress text; success ends with `Next: assay validate --config eval.yaml --trace-file traces/hello.jsonl`. A failing run writes partial progress text, not the fatal diagnosis. | No machine report, `reason_code`, or `next_step` on stdout: [gap #2161](https://github.com/Rul1an/assay/issues/2161). |
| 4. Policy validation | `invocation cwd` | `assay policy validate --input <policy> --format json` | Valid `0`; malformed `2`. | Both paths parse as `assay.run_summary.v1`; valid has exit `0` and an empty reason, while malformed carries `E_POLICY_PARSE`. | Malformed policy is exit `2` and names the failing policy in a concrete `assay policy validate` next step. |
| 5. Evaluation result | `invocation cwd` | `assay run --config eval.yaml --trace-file traces/hello.jsonl --format json` | All tests pass `0`; completed run with failed tests `1`. | Both completed outcomes parse as `assay.run_report.v1`; failed results carry `status: fail` inside `results`. | Exit `1` is a completed results report, not an early-failure diagnosis; `reason_code` and `next_step` are absent by design. |
| 6. Protected action | `examples/privileged-action-gate` | `assay-mcp-server proxy-enforce --upstream-command <python> --upstream-arg -u --upstream-arg mock_github_mcp.py --enforce-policy policies/no-allowance.yaml --declared-mcp-manifest baseline-approved.json` | Policy-denied call after stdin closes `0`; startup input failure `1`. | The denied `tools/call` response pins `error.code: -32042`, `error.data.origin: assay-proxy`, and `error.data.reason: no_declared_allowance`. | Policy denial is not a process failure. A missing enforcement policy fails startup with empty stdout and no stable reason/next-step object: [gap #2163](https://github.com/Rul1an/assay/issues/2163). |
| 7. Evidence inspection | `invocation cwd` | `assay evidence show <bundle> --format json` | Valid `0`; integrity failure `2`. | Success parses as an object containing `manifest` and `events`. Integrity failure produces no stdout. | No JSON failure report, `reason_code`, or `next_step`: [gap #2164](https://github.com/Rul1an/assay/issues/2164). |
| 8. Offline profile verification | `invocation cwd` | `assay evidence verify-privileged-mcp-action <bundle> --format json` | Valid `0`; integrity or profile failure `2`. | Both paths parse as `assay.privileged_mcp_action.verify.report.v0`. Success has `bundle_integrity: pass` and `verdict: valid`; tamper has `bundle_integrity: fail`, a bounded finding, and no verdict. | The failure report has no registered `reason_code` or actionable `next_step`: [gap #2165](https://github.com/Rul1an/assay/issues/2165). |
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
export CARGO_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/assay-target-2154.XXXXXX")"
cargo test -p assay-cli --test agent_golden_path_contract
cargo test -p assay-mcp-server --test agent_golden_path_contract
```

These tests run the Cargo-built binaries through `CARGO_BIN_EXE_*`; they do not
read an unrelated Assay executable from `PATH`. The reference upstream remains
the committed Python fixture used by the protected-action scenario, which
requires `python3` on Unix-like hosts or `python` on Windows.
