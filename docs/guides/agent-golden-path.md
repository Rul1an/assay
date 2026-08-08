# Agent Golden Path: stdout and Exit Codes

This is the machine-facing description of the same install, protect, inspect,
verify, and project journey tracked in
[#1975](https://github.com/Rul1an/assay/issues/1975). It records what the
current commands actually expose to a caller that reads stdout and the process
exit code. It is not a claim that every row already has the desired shape.

The table is driven by binary-level tests in `assay-cli` and
`assay-mcp-server`. Paths in angle brackets are caller-supplied; the tests
replace them with temporary files or committed fixtures. Structured-output
checks pin schemas and stable fields rather than timestamps, host paths, or
free-form prose.

| step | command | exit code | stdout | on failure |
|---|---|---|---|---|
| 1. Install check | `assay version` | `0` | One `MAJOR.MINOR.PATCH` line. | A missing or unstartable binary is a host spawn failure: no Assay process runs, so Assay produces no stdout or exit code. |
| 2. Preflight | `assay doctor --format json` | Success `0`; invalid explicit config `1`. | Parses as `assay.doctor_report.v0`. A config failure remains JSON and carries `config_error.code: E_CFG_PARSE`. | `reason_code` and `next_step` are absent, and the exit is outside the frozen config/usage class: [gap #2160](https://github.com/Rul1an/assay/issues/2160). |
| 3. Starter files | `assay init --preset dev --hello-trace` | Success `0`; unknown preset `2`. | Human progress text; success ends with `Next: assay validate --config eval.yaml --trace-file traces/hello.jsonl`. A failing run writes partial progress text, not the fatal diagnosis. | No machine report, `reason_code`, or `next_step` on stdout: [gap #2161](https://github.com/Rul1an/assay/issues/2161). |
| 4. Policy validation | `assay policy validate --input policy.yaml` | Valid `0`; malformed `2`. | Empty on both paths. | The diagnosis, registered reason, and next step are absent from stdout: [gap #2162](https://github.com/Rul1an/assay/issues/2162). |
| 5. Protected action | `assay-mcp-server proxy-enforce <args>` | A policy-denied call is a JSON-RPC result and the process exits `0` after stdin closes; startup input failure exits `1`. | The denied `tools/call` response pins `error.code: -32042`, `error.data.origin: assay-proxy`, and `error.data.reason: no_declared_allowance`. | Policy denial is not a process failure. A missing enforcement policy fails startup with empty stdout and no stable reason/next-step object: [gap #2163](https://github.com/Rul1an/assay/issues/2163). |
| 6. Evidence inspection | `assay evidence show <bundle> --format json` | Valid `0`; integrity failure `2`. | Success parses as an object containing `manifest` and `events`. Integrity failure produces no stdout. | No JSON failure report, `reason_code`, or `next_step`: [gap #2164](https://github.com/Rul1an/assay/issues/2164). |
| 7. Offline profile verification | `assay evidence verify-privileged-mcp-action <bundle> --format json` | Valid `0`; integrity or profile failure `2`. | Both paths parse as `assay.privileged_mcp_action.verify.report.v0`. Success has `bundle_integrity: pass` and `verdict: valid`; tamper has `bundle_integrity: fail`, a bounded finding, and no verdict. | The failure report has no registered `reason_code` or actionable `next_step`: [gap #2165](https://github.com/Rul1an/assay/issues/2165). |
| 8. SARIF projection | `assay-mcp-server enforcement-sarif --input <decisions.ndjson> --output -` | Valid input `0`; malformed non-empty NDJSON currently also `0`. | Valid input produces SARIF `2.1.0`. Malformed lines are currently discarded and can produce a clean report with zero results. | This is fail-open, not a successful empty projection: [gap #2166](https://github.com/Rul1an/assay/issues/2166). No reason or next step is emitted. |

## Reading the Contract

- An exit code is a process result. A policy denial in step 5 is instead a
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
CARGO_TARGET_DIR=/tmp/assay-target-2154 \
  cargo test -p assay-cli --test agent_golden_path_contract
CARGO_TARGET_DIR=/tmp/assay-target-2154 \
  cargo test -p assay-mcp-server --test agent_golden_path_contract
```

These tests run the Cargo-built binaries through `CARGO_BIN_EXE_*`; they do not
read an unrelated executable from `PATH`.
