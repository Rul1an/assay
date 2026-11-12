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

This source tree declares Assay `6.25.0` (`v6.25.0`).
This journey is pinned to Assay `6.24.0` ([`v6.24.0`](https://github.com/Rul1an/assay/releases/tag/v6.24.0)).
Install the CLI from a verified channel, then require `assay version` to print `6.24.0` before using the table below. Behavior merged after that tag is `Unreleased` and is not part of this release claim.

Upgrade by installing a newer explicit release and re-running all nine steps.
Roll back by reinstalling `v6.24.0` from the GitHub release assets and re-running the same journey.
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
| 6. Protected action | `examples/privileged-action-gate` | `assay-mcp-server proxy-enforce --upstream-command <python> --upstream-arg -u --upstream-arg mock_github_mcp.py --enforce-policy policies/no-allowance.yaml --declared-mcp-manifest baseline-approved.json` | Policy-denied call after stdin closes `0`; startup input failure `1`. | The denied `tools/call` response pins `error.code: -31999`, `error.data.origin: assay-proxy`, and `error.data.reason: no_declared_allowance`. Exit 0 after stdin closes is the process outcome, not proof of correct proxy operation or enforcement. The denial response does not by itself prove whether an upstream or protected external action executed or did not execute. | Policy denial is not a process failure. A missing enforcement policy fails startup with empty stdout and one JSON `startup_failure` event on stderr, including `reason_code: proxy_enforce_policy_invalid` and an actionable `next_step`. |
| 7. Evidence inspection | `invocation cwd` | `assay evidence show --format json -- <bundle>` | Valid `0`; Valid with verification disabled `0`; integrity failure `2`; unreadable bundle `2`; format-contract failure `2`. | Success parses as an object containing `manifest`, `events`, `verify_mode`, and `content_hash_scope`; the registered values are `enabled` and `disabled`, with `--no-verify` producing `disabled`. A recorded-value mismatch parses as `assay.run_summary.v1` with `E_EVIDENCE_INTEGRITY`; an unreadable path uses `E_EVIDENCE_UNREADABLE`; a typed `Contract*` defect uses `E_EVIDENCE_CONTRACT`. | Only the four verifier codes that establish a recorded-value mismatch map to `E_EVIDENCE_INTEGRITY`; I/O, gzip, and tar failures use `E_EVIDENCE_UNREADABLE`. Typed `Contract*` failures encountered while opening the bundle publish `E_EVIDENCE_CONTRACT` with a bounded prose `next_step`. Event-line deserialization plus LIMIT/PATH and PROFILE, where they apply, remain exit `2` with empty stdout until typed on this command. |
| 8. Offline profile verification | `invocation cwd` | `assay evidence verify-privileged-mcp-action <bundle> --format json` | Valid `0`; integrity or profile failure `2`. | Both paths parse as `assay.privileged_mcp_action.verify.report.v0`. Success has `bundle_integrity: pass` and `verdict: valid` and omits diagnosis. Tamper has `bundle_integrity: fail`, a bounded finding, no verdict, and `E_EVIDENCE_INTEGRITY`. | A recorded-value mismatch publishes `E_EVIDENCE_INTEGRITY`. A typed `Contract*` defect publishes `E_EVIDENCE_CONTRACT`. Untyped I/O and archive-read failures publish `E_EVIDENCE_UNREADABLE`. A stage-1 pass whose profile verdict is invalid publishes `E_EVIDENCE_PROFILE_INVALID`. Ceiling refusals and `SecurityPathTraversal` publish `E_EVIDENCE_LIMIT_EXCEEDED` and `E_EVIDENCE_PATH_REJECTED`. Success omits both diagnostic fields. `findings[].detail` may retain the caller argv path. Unreadable `next_step` is shell-free caller-argv (concrete JSON `Run argv` with `--` and the caller path), not a shell string. Other owned codes stay prose. Without `--profile-version` the verifier applies the v0 interpreter and reports `profile_selection: default`. A bundle carries no profile id, so the intended version comes from the producer contract or the caller's requirement, never from whichever selection passes. Assay 6.9.0 keeps the reissue text for profile-invalid observations. From Assay 6.10.0: When the only violations are observation records that another shipped interpreter of this profile recognizes, and no observation of the selected interpreter is present, the profile-invalid `next_step` asks for that interpreter check instead of reissue and names no alternative version. Mixed observation versions keep the reissue text. |
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

## Own-server path: wrap session to an offline-verified package

This experimental Unix path is available in **Assay v6.21.0**. It is separate
from the nine release-pinned steps above. A user who wraps their own stdio MCP server can take a decision log from
`assay mcp wrap --decision-log` to a package someone else verifies offline.
The commands below were run against
[`examples/mcp-quickstart/mock_server.py`](../../examples/mcp-quickstart/mock_server.py)
on the source tree. The command is included in v6.21.0 Unix archives;
v6.20.0 archives do not include it. The optional `--request-receipts` prepare
extension described below is included in v6.22.0 and later; v6.21.0 does not have it.

`assay evidence prepare-incident-package -h` says it derives and exports an
incident package from wrap-session outputs **(Unix)**. The Windows build prints
that the command currently requires Unix regular-file admission and atomic
publication, and exits 2.

This is not a count of all calls, not completeness of capture, not proof of
what the server did, and an allow is not proof a human accepted. Server asks
(sampling, elicitation) are kept only with `--request-receipts-out`, in that
sidecar. They are excluded by default; the v6.22.0 `--request-receipts PATH`
prepare option retains that whole file as context without new joins or completeness claims.

### 1. Wrap and keep the decision log

`--decision-log` requires `--event-source`. `--decision-log-new` refuses an
existing path so this run is not appended onto another session; mixed session
`source` values refuse later as `input_shape`. The included mock writes
`.assay/quickstart/mock-invocation.json` relative to the working directory.

```bash
mkdir -p .assay/quickstart
umask 077

python3 - <<'PY' | assay mcp wrap \
  --policy examples/mcp-quickstart/policy.yaml \
  --verbose \
  --event-source assay://quickstart/own-server \
  --decision-log decisions.ndjson \
  --decision-log-new \
  -- python3 -u examples/mcp-quickstart/mock_server.py
import json, sys
for request in [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "own-server", "version": "1"}}},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "read_file", "arguments": {"path": "/tmp/assay-demo/safe.txt"}}},
]:
    sys.stdout.write(json.dumps(request, separators=(",", ":")) + "\n")
PY
```

Stderr from that wrap (exit 0) on this tree:

```text
[assay] loading policy from examples/mcp-quickstart/policy.yaml
[assay] wrapping command: python3 ["-u", "examples/mcp-quickstart/mock_server.py"]
[assay] ALLOW read_file
```

The decision log holds one `assay.tool.decision` CloudEvent for `read_file`
with `decision` `allow`. That allow is a policy decision on the call that
crossed wrap, not proof the mock (or any server) did a side effect, and not
proof a human accepted the call.

### 2. Agree a context before preparation

The receiver will pass the same `ContextInput` JSON to
`verify-incident-package`. Agree it before `prepare-incident-package` writes
the assessment. `as_of` is explicit. Validity is
`valid_from <= as_of <= valid_until`; it is not judged against the wall clock,
and there is no freshness claim. This run used `as_of` `2026-09-08T00:00:00Z`
and `valid_until` `2026-09-09T00:00:00Z` on 2026-11-02, and verification still
passed.

Without `--context`, both commands use the same receiver default (SPEC example
A1; `verification_context_sha256`
`919787b66b4dc0688944d7c76218c6da168890f8251985ccfb12e2a92cfcb814`). The file
below is that default with a selected `outer_bytes` of `65536` (below the hard
maximum of `268435456`) and an invocation label. Save it as `context.json`.

```json
{
  "schema_pin": "4453f216089543780bfecc4295cc4a61462fdc585b88d1e35b7d1aba79716b4a",
  "limits": {
    "outer_bytes": 65536,
    "members": 128,
    "outer_path_bytes": 80,
    "object_bytes_total": 251658240,
    "inputs": 64,
    "inner_compressed_bytes": 104857600,
    "decoded_bytes_total": 536870912,
    "nested_archive_depth": 1,
    "inventory_bytes": 1048576,
    "assessment_bytes": 16777216,
    "policy_bytes": 1048576,
    "transcript_bytes": 16777216,
    "health_bytes": 1048576,
    "envelope_bytes": 33554432,
    "key_bytes": 16384,
    "retained_report_bytes": 16777216,
    "inner_manifest_bytes": 10485760,
    "inner_events_bytes": 524288000,
    "records": 100000,
    "record_bytes": 1048576,
    "inner_path_bytes": 256,
    "json_depth": 64,
    "json_key_bytes": 256,
    "non_payload_string_bytes": 65536,
    "payload_string_bytes": 1048576,
    "references": 100000,
    "reference_depth": 1,
    "diagnostic_bytes": 4096,
    "fresh_result_bytes": 16777216
  },
  "keys": [],
  "expectations": [],
  "applicability": [],
  "as_of": "2026-09-08T00:00:00Z",
  "valid_from": "2026-09-08T00:00:00Z",
  "valid_until": "2026-09-09T00:00:00Z",
  "invocation": {
    "tool": "assay-cli",
    "version": "6.20.0",
    "options": ["evidence", "verify-incident-package"]
  }
}
```

### 3. Prepare, then hand over package plus context

```bash
assay evidence prepare-incident-package \
  --decision-log decisions.ndjson \
  --context context.json \
  --out incident.tar
```

Exit 0. Stdout is one JCS `assay.incident.verify.v1` object. This run:

- `outcome`: `package_verified`
- `verification_context_sha256`: `cd7c49a978c031dbc4520bd1acfe7ea975c3899a9c27b3ce626217a7b7863cd8`
- `counts.units` / `counts.examined`: `1`
- `non_claims`: `no_activity_completeness`, `no_provider_outcome`, `no_automatic_trust`

That digest is bound into `assessment.json` as `verification_context_sha256`.
Hand the receiver `incident.tar` and the same `context.json`. A different
context is a different verification; loosening `outer_bytes` back to the hard
maximum on this package refused with `stale_assessment`.

### 4. Receiver verifies offline

```bash
assay evidence verify-incident-package incident.tar --context context.json
```

Exit 0. This binary writes pretty JSON (not JCS) with the same
`package_verified` outcome and the same
`verification_context_sha256`. After that, `tar -xOf incident.tar assessment.json`
showed every surface observation as `unknown` (`tool_call`, `filesystem`,
`network`, `process`, `transcript_join`), with empty `joins` and `claims`.
Unknown grades on a proxy-only wrap session are the evidence limit, not a
defect: wrap recorded tool-call decisions, not kernel, filesystem, network, or
transcript-join observation.

### Receiver refusals

Keep the command's exit status, JSON `reason`, and stderr. A matching checksum
of a refused archive is not a pass.

| Receiver input | `reason` on this tree |
|---|---|
| Object bytes changed (path no longer the SHA-256 of the member) | `input_shape` |
| Object member removed, or a well-formed `assessment.json` whose bytes no longer match the inventory digest | `digest` |
| Context `expectations` name the wrap log with the wrong SHA-256 | `expectation_mismatch` |
| Selected `limits.outer_bytes` one below the package size (`10751` vs `10752`) | `resource_selected` |

`input_shape` is a broad class, not a diagnosis of the precise defect. See
[Prepare an incident package from a wrap session](../reference/cli/evidence.md#prepare-an-incident-package-from-a-wrap-session)
and [Incident Package v1](../architecture/SPEC-Incident-Package-v1.md).
