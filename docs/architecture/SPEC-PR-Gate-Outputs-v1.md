# PR Gate Output Contracts Specification v1

**Status:** Draft
**Version:** 1.0.1-draft
**Date:** 2026-02
**ADR:** [ADR-019: PR Gate 2026 SOTA](./ADR-019-PR-Gate-2026-SOTA.md)
**Related:** [DX-IMPLEMENTATION-PLAN-legacy](../archive/DX-IMPLEMENTATION-PLAN-legacy.md), [SPEC-GitHub-Action-v2.1](./SPEC-GitHub-Action-v2.1.md)

---

## 1. Overview

This specification defines the **output contracts** for the Assay PR gate: the blessed flow outputs (`junit.xml`, `sarif.json`, `summary.json`), exit and reason code semantics, SARIF constraints for GitHub compatibility, and the requirement that every non-zero exit provides a suggested next step. Implementations of `assay ci` and `assay run` (when used as the CI entrypoint) MUST conform to this spec so that CI consumers and the GitHub Action get predictable, machine-readable results.

### Design Principles

- **PR-native** — Outputs integrate with GitHub (JUnit → test annotations, SARIF → Security tab, Check Run Summary) without custom glue.
- **Stable and versioned** — summary.json carries a schema_version so consumers can detect and adapt to changes.
- **Machine-readable nuance** — Exit codes stay coarse (0/1/2/3); reason codes in summary.json and console provide stable, fine-grained semantics without breaking exit-code scripts.
- **Upload-safe** — SARIF stays within GitHub limits (size, result count) so upload never fails randomly; every result has at least one location.

---

## 2. Blessed Flow Outputs

When `assay ci` (or the equivalent run invoked by the blessed workflow) completes, it MUST produce the following artifacts in the configured output directory (default: `.assay/reports` or equivalent).

| Artifact       | Required | Description |
|----------------|----------|-------------|
| `junit.xml`    | Yes      | JUnit XML format; test cases with `<failure>` for Fail/Error; compatible with GitHub test reporting and JUnit reporter actions. |
| `sarif.json`   | Yes      | SARIF 2.1.0; see §5 for location and truncation rules. |
| `summary.json` | Yes      | Machine-readable run summary; see §3 for schema. |

**Normative:** The blessed entrypoint is `assay ci`. The same three outputs MUST be produced so that one local command reproduces exact CI behaviour.

---

## 3. summary.json Schema

### 3.1 Required Top-Level Fields

| Field                 | Type    | Required | Description |
|-----------------------|---------|----------|-------------|
| `schema_version`     | integer | **Yes**  | Version of this summary schema. MUST be `1` for this spec. Increment when adding or changing fields in a backward-incompatible way. |
| `reason_code_version`| integer | **Yes**  | Version of the reason code registry. MUST be present. MUST equal `1` in Outputs-v1. Future changes to the reason code set use this version. Consumers MUST branch on `(reason_code_version, reason_code)` for semantics; exit code is coarse transport only. Consumers MUST treat unknown versions as "compat required" (fail closed or fallback parsing). |
| `exit_code`           | integer | **Yes**  | Process exit code: 0 = pass, 1 = test failure, 2 = config/user error, 3 = infra/judge unavailable. See §4. |
| `reason_code`         | string  | **Yes**  | Stable machine-readable code when exit_code ≠ 0; e.g. `E_TRACE_NOT_FOUND`, `E_JUDGE_UNAVAILABLE`. See §5. When exit_code is 0, MAY be empty string or a designated success code (e.g. OK); empty is allowed and common. |
| `message`             | string  | No       | Human-readable one-line description of outcome. |
| `next_step`           | string  | No       | Single suggested command or hint when exit_code ≠ 0; e.g. "Run: assay doctor --config ...", "See: assay explain ...". See §7. |

### 3.2 Provenance (Artifact Auditability)

Every summary.json MUST include a top-level **`provenance`** object with the following fields so that gates remain auditable (ADR-019 P0.4).

| Field                 | Type   | Required | Description |
|-----------------------|--------|----------|-------------|
| `assay_version`       | string | **Yes**  | Assay CLI version that produced this run (e.g. `"2.12.0"`). |
| `verify_mode`         | string | **Yes**  | `"enabled"` or `"disabled"`. When `"disabled"`, indicates signature verification was turned off (UNSAFE). |
| `policy_pack_digest`  | string | No       | Digest of policy/pack used (e.g. `sha256:...`). |
| `baseline_digest`     | string | No       | Digest of baseline used for comparison, if applicable. |
| `trace_digest`        | string | No       | Digest of trace input, if applicable (optional for privacy/size). |
| `replay`              | boolean| No       | `true` when this output was produced by replay from a bundle. |
| `bundle_digest`       | string | No       | SHA256 digest of the replay bundle archive used for this run. |
| `replay_mode`         | string | No       | Replay mode when `replay=true`: `"offline"` or `"live"`. |
| `source_run_id`       | string | No       | Optional original run id carried into replay provenance. |

**Normative:** If the run was executed with `--no-verify`, `verify_mode` MUST be `"disabled"`.
When replay is used, producers SHOULD set `replay=true` and include `bundle_digest` and `replay_mode`.

### 3.3 Results Summary (Optional but Recommended)

A top-level **`results`** object MAY contain:

| Field     | Type    | Required | Description |
|-----------|---------|----------|-------------|
| `passed`  | integer | No       | Count of tests passed. |
| `failed`  | integer | No       | Count of tests failed. |
| `warned`  | integer | No       | Count of tests with Warn/Flaky (depends on strict mode). |
| `skipped` | integer | No       | Count of tests skipped (e.g. cache hit). |
| `total`   | integer | No       | Total test count. |

A top-level **`performance`** object MAY contain `total_duration_ms` (integer, milliseconds). Future versions MAY add `slowest_tests`, `cache_hit_rate`, `phase_timings` (see ADR-019 / DX-IMPLEMENTATION-PLAN). Consumers MUST ignore unknown top-level keys.

### 3.3.1 Seeds (E7.2 – Replay Determinism)

A top-level **`seeds`** object (summary.json) and top-level **`seed_version`**, **`order_seed`**, **`judge_seed`** (run.json) SHALL be present for schema stability. On early-exit (e.g. trace not found, config fail), seeds may be `null` when unknown; `seed_version` SHALL still be present.

| Field          | Type            | Required | Description |
|----------------|-----------------|----------|-------------|
| `seed_version` | integer         | **Yes**  | Version of the seed schema. MUST be `1` for Outputs-v1. Consumers MUST branch on `seed_version` when interpreting seeds. |
| `order_seed`   | string or null  | **Yes**  | Decimal u64 encoded as string to avoid JSON number precision loss; null on early-exit when unknown. |
| `judge_seed`   | string or null  | **Yes**  | Decimal u64 encoded as string; MAY be null until judge-level seeding is implemented (E9); consumers MUST handle null. |
| `sampling_seed`| integer         | No       | Optional: determinism for telemetry sampling (reserved for future use). |

**Normative:** run.json (extended and minimal) and summary.json SHALL include `seed_version`; order_seed and judge_seed SHALL be present (string or null). Seeds MUST be encoded as decimal strings (or null) to avoid precision loss in JSON consumers (e.g. JS/TS safe for u64 > 2^53). CLI console SHALL print one line: `Seeds: seed_version=1 order_seed=… judge_seed=…` so CI job summaries can show them for replay.

### 3.3.2 Judge Metrics (E7.3)

When the run had judge evaluations, a top-level **`judge_metrics`** object MAY be present with low-cardinality reliability metrics:

| Field               | Type    | Required | Description |
|---------------------|---------|----------|-------------|
| `abstain_rate`      | number  | No       | Fraction of judge evaluations that returned Abstain (uncertain). |
| `flip_rate`         | number  | No       | Fraction of evaluations where order was swapped and outcome differed. (Implementation may use a proxy: swapped and non-unanimous agreement, when the judge does not record whether the pass/fail verdict would have differed under the other ordering.) |
| `consensus_rate`    | number  | No       | Fraction of evaluations where all samples agreed. |
| `unavailable_count` | integer | No       | Count of runs where judge was unavailable (infra/transport); not counted toward abstain_rate. |

**Normative:** Judge unavailable (transport/infra) MUST NOT be counted as Abstain; use `unavailable_count` for that.

**Implementation note (unavailable_count):** Implementations may use message heuristics (e.g. timeout, 5xx, rate limit, network) on Error-status rows to classify infra failures. Abstain (uncertain verdict) is never counted as unavailable. Prefer standardised reason codes or an explicit infra_class field when available.

**Implementation note (flip_rate):** The spec defines flip_rate as “order was swapped and outcome differed”. When the judge does not record whether the pass/fail verdict would have differed under the other ordering, implementations may use a heuristic proxy (e.g. swapped and non-unanimous agreement). This proxy does not guarantee that the verdict actually flipped; it indicates order may have affected the outcome. When present, run.json and the CLI console SHALL expose judge metrics so CI can display them.

### 3.4 Example (Minimal)

```json
{
  "schema_version": 1,
  "reason_code_version": 1,
  "exit_code": 0,
  "reason_code": "",
  "provenance": {
    "assay_version": "2.12.0",
    "verify_mode": "enabled"
  },
  "results": {
    "passed": 10,
    "failed": 0,
    "total": 10
  },
  "performance": {
    "total_duration_ms": 1234
  }
}
```

### 3.5 Example (Non-Zero with Next Step)

```json
{
  "schema_version": 1,
  "reason_code_version": 1,
  "exit_code": 2,
  "reason_code": "E_TRACE_NOT_FOUND",
  "message": "Trace file not found: traces/ci.jsonl",
  "next_step": "Run: assay doctor --config ci-eval.yaml --trace-file traces/ci.jsonl",
  "provenance": {
    "assay_version": "2.12.0",
    "verify_mode": "enabled"
  }
}
```

---

## 4. Exit Code Registry

Exit codes are **coarse** and MUST NOT be redefined in a breaking way. Reason codes (§5) carry the nuance.

| Exit Code | Meaning                  | Typical reason_codes |
|-----------|--------------------------|----------------------|
| 0         | All tests passed         | (none)               |
| 1         | One or more tests failed | (test-level codes)   |
| 2         | Configuration / user error| E_CFG_PARSE, E_TRACE_NOT_FOUND, E_MISSING_CONFIG, etc. |
| 3         | Infra / judge unavailable| E_JUDGE_UNAVAILABLE, E_RATE_LIMIT, E_PROVIDER_5XX, E_TIMEOUT |

**Normative:** Judge failures (rate limit, provider 5xx, timeout) MUST map to exit code 3. Behaviour for security vs quality suites is policy-driven (fail-closed vs degrade/skip) per ADR-003/ADR-004; the exit code alone does not change.

**Compatibility:** Historically, some documentation used exit 3 for "trace file not found". Under this spec, trace-not-found is exit 2 with reason_code E_TRACE_NOT_FOUND. Implementations MAY support a compatibility mode (e.g. `--exit-codes=v1`) that preserves the old mapping for a documented deprecation period.

---

## 5. Reason Code Registry

Reason codes are **stable, machine-readable** strings. CI and scripts MAY branch on `reason_code` in summary.json. New codes MUST be added in a backward-compatible way (new string values); existing codes MUST NOT be removed or repurposed without a schema_version bump and migration notes.

### 5.1 Config / User Error (exit_code 2)

| Code                | Description |
|---------------------|-------------|
| E_CFG_PARSE         | Config file parse error (YAML/JSON). |
| E_TRACE_NOT_FOUND   | Trace file or path not found. |
| E_MISSING_CONFIG    | Required config file missing. |
| E_BASELINE_INVALID  | Baseline file invalid or missing. |
| E_POLICY_PARSE      | Policy file parse error. |
| E_REPLAY_MISSING_DEPENDENCY | Replay missing required offline dependency (e.g. uncached judge/cassette input). |
| E_INVALID_ARGS      | Command-line arguments are invalid or mutually inconsistent. Registered late: the code has been emitted since before this registry existed and is asserted by `crates/assay-cli/tests/contract_run_ci_parity.rs`, so its absence here was a gap in the spec, not a new code. |
| E_REPLAY_LIMIT_EXCEEDED | Replay bundle refused by an ingest ceiling during bounded ingest, before replay execution. The source, decode, member, path, entry-count and manifest-depth ceilings apply at different points of the read, so this is not a claim that nothing was parsed. It establishes only that a configured budget was exceeded; whether the bundle is otherwise valid is unresolved, because the read stopped. Adjusting the budget or supplying a smaller bundle is a legitimate response. Distinct from `E_CFG_PARSE`, which is a malformed-input finding. Carries no verdict: no test ran. |

### 5.2 Infra / Judge Unavailable (exit_code 3)

| Code                | Description |
|---------------------|-------------|
| E_JUDGE_UNAVAILABLE | Judge service unavailable or returned error. |
| E_RATE_LIMIT        | Judge/provider rate limit hit. |
| E_PROVIDER_5XX      | Judge/provider returned 5xx. |
| E_TIMEOUT           | Judge or dependency timed out. |
| E_NETWORK_ERROR     | Connection refused, DNS failure, or other transport-level failure reaching a provider. Registered late for the same reason as `E_INVALID_ARGS`. |

### 5.3 Test Failure (exit_code 1)

| Code                | Description |
|---------------------|-------------|
| E_TEST_FAILED       | One or more tests failed and no single dominant reason is reported. |
| E_JUDGE_UNCERTAIN   | The judge abstained. Whether that fails the run is policy-dependent per ADR-004. |
| E_POLICY_VIOLATION  | A policy check blocked a tool call. Also a member of `assay_core::errors::diagnostic::codes`, where it reaches a SARIF `ruleId`; see `REASON-CODE-VOCABULARIES.md` surface 1. |
| E_ARG_SCHEMA        | Argument schema validation failed. |
| E_SEQUENCE_VIOLATION | A sequence assertion failed. |

This section used to be prose naming `E_ARG_SCHEMA`, `E_SEQUENCE_VIOLATION` and `E_TEST_FAILED` as examples. The normative rule below requires `reason_code` to be *one of the registered values*, and an example in prose is not a registration -- so `E_JUDGE_UNCERTAIN` and `E_POLICY_VIOLATION`, both emittable, were unregistered while the rule said they could not be. The table is the registration.

**Normative:** When exit_code ≠ 0, summary.json MUST set `reason_code` to one of the registered values (or a documented extension). Implementations MUST NOT leave reason_code empty when exit_code ≠ 0.

### 5.4 Reserved

Declared somewhere in the implementation and currently constructed by nothing. They are listed rather than deleted, because §175 above forbids removing a registered code without a `schema_version` bump and migration notes -- and a code that a consumer once saw is a code a consumer may still branch on.

Reserved means: an implementation MAY begin emitting it without a version bump, because it is registered here; and a consumer MUST NOT assume it will never appear.

| Code | Declared in | Note |
|------|-------------|------|
| E_BASELINE_INVALID | `ReasonCode::EBaselineInvalid` | Registered in §5.1. Nothing constructs the variant. |
| E_POLICY_PARSE | `ReasonCode::EPolicyParse` | Registered in §5.1. Nothing constructs the variant. |
| E_ARG_SCHEMA | `ReasonCode::EArgSchema` | The *variant* is dead; the string is live, originated by `assay_core::policy_engine:102` and forwarded into a `Diagnostic`. Two producers, one code. |
| E_SEQUENCE_VIOLATION | `ReasonCode::ESequenceViolation` | As above, originated at `policy_engine:302`. |
| W_BASE_FINGERPRINT | `codes::W_BASE_FINGERPRINT` | Warning severity, so it never reaches `reason_code`. Constructed nowhere. |
| W_CACHE_CONFUSION | `codes::W_CACHE_CONFUSION` | As above. |
| E_CFG_SCHEMA_UNKNOWN_FIELD, E_POLICY_SCHEMA_UNKNOWN_FIELD, E_CFG_REF_MISSING, E_BASELINE_NOT_FOUND, E_BASELINE_SUITE_MISMATCH, E_ARG_PATTERN_BLOCKED, E_CONSTRAINT_MISSING, E_EXEC_DENIED, E_PATH_SCOPE_VIOLATION, E_SIGNATURES_DISABLED, E_TOOL_DESC_SUSPICIOUS, E_TOOL_POISONING_PATTERN, E_TRACE_LEGACY_FUNCTION_CALL, E_TRACE_SCHEMA_DRIFT, E_TRACE_SCHEMA_INVALID, UNKNOWN_TOOL | matched in `assay_core::agentic::builder` | Remediation branches keyed on codes no producer in this workspace constructs. Measured: zero production construction sites each, outside the builder's own tests. |
| E_TOOL_DENIED, E_TOOL_NOT_ALLOWED, MCP_TOOL_DENIED, MCP_TOOL_NOT_ALLOWED | matched in `assay_core::agentic::builder` | The first two are live, but on another surface: `assay_core::mcp::policy` and `assay_metrics::args_valid_next` write them into MCP decision records and metric details, neither of which becomes a `Diagnostic`. The `MCP_`-prefixed pair is constructed nowhere. |

`codes::E_POLICY_VIOLATION` is dead as a constant while `E_POLICY_VIOLATION` the string is live and registered in §5.3, reachable through `ReasonCode::EPolicyViolation`.

A remediation matcher keyed on a reserved code is a branch that cannot fire today. Recording it here is not an endorsement of keeping it -- it makes the dead branch visible so #2028 can decide, rather than leaving it to be rediscovered.

The scale is worth stating plainly, because it was measured rather than estimated. `agentic::builder` matches 20 codes that are not `codes::` members. Two of them -- `E_ARG_SCHEMA` and `E_SEQUENCE_VIOLATION` -- can reach it, forwarded from the policy engine. The other 18 cannot: nothing in the workspace constructs them as a `Diagnostic.code`. The remediation surface is keyed on a vocabulary that was planned and never wired up.

`E_POLICY_MISSING_TOOL`, `E_POLICY_REGEX_INVALID` and `E_SCHEMA_COMPILE` are **not** reserved: they are live, constructed by `assay_core::policy_engine`, and reach a published SARIF artifact. They are registered in §5.5.

### 5.5 Policy-Engine Verdict Codes

`assay_core::policy_engine` builds a `Verdict.reason_code`, and `assay_core::validate` forwards it verbatim into a `Diagnostic` whose `code` becomes a SARIF `ruleId`. They are therefore public identifiers on a published artifact, and were registered nowhere.

| Code | Description |
|------|-------------|
| E_ARG_SCHEMA | Tool arguments failed their JSON Schema. Also §5.3. |
| E_SEQUENCE_VIOLATION | A tool-call sequence assertion failed. Also §5.3. |
| E_POLICY_MISSING_TOOL | The policy has no entry for the tool being called. |
| E_POLICY_REGEX_INVALID | A policy constraint's regular expression failed to compile. |
| E_SCHEMA_COMPILE | A policy's JSON Schema failed to compile. |

`OK` is the `reason_code` of a non-blocked verdict. It never reaches a `Diagnostic`, because only `VerdictStatus::Blocked` is forwarded, and it is not a reason code in the sense of this registry.

The MCP policy engine has its own `E_`-prefixed vocabulary (`E_TOOL_NOT_ALLOWED`, `E_TOOL_DENIED`, `E_TOOL_DRIFT`, `E_RATE_LIMIT`) that reaches MCP decision records rather than `summary.json` or SARIF. It is out of scope for this registry; see `REASON-CODE-VOCABULARIES.md` surface 6.

---

## 6. SARIF Contract (GitHub Compatibility)

SARIF produced for GitHub Code Scanning MUST satisfy the following so that `upload-sarif` does not reject the file.

### 6.1 Schema and Version

- SARIF version MUST be `"2.1.0"`.
- Schema URI MUST be the official SARIF 2.1.0 JSON schema.

### 6.2 Location Requirement

- **Every result MUST have at least one location.** If no file/line is available, the producer MUST emit a **synthetic location** (e.g. URI `assay.yaml`, `policy.yaml`, or the config path). GitHub's upload can fail with "expected at least one location" when a result has an empty `locations` array.

**Normative:** Contract tests MUST validate that every result in the generated SARIF has `locations` length ≥ 1.

### 6.3 Truncation (Size and Result Limits) (E2.3)

- GitHub enforces limits on SARIF upload (e.g. max size gzipped, max number of results). Producers MUST **truncate** results when limits would be exceeded, and MUST add a clear indication that results were omitted (e.g. in the run description or a dedicated message: "N results omitted due to GitHub upload limits").
- Truncation strategy: keep top N results by severity (e.g. error first, then warning). N and the exact message are implementation-defined but MUST be documented. Truncation MUST be **deterministic** (same run → same selection); selection order: blocking (Fail/Error) first, then warning-level, then stable sort (e.g. by test_id). **Eligibility:** only SARIF-eligible results (e.g. Fail, Error, Warn, Flaky, Unstable) count toward the limit. Define **eligible_total** as the count of SARIF-eligible results before truncation; **included** as the number of results actually written in the SARIF run; then `omitted_count` = eligible_total − included.
- **SARIF run-level metadata when truncated:** `runs[].properties.assay` MUST be present when truncation was applied:
  - `truncated` (boolean): `true`
  - `omitted_count` (integer): number of eligible results omitted
- **summary.json and run.json — nested `sarif` object:** When SARIF was truncated, summary.json and run.json MAY include a top-level **`sarif`** object. Schema when present:
  - `sarif` (object, optional): present only when truncation occurred (recommended to reduce noise).
  - `sarif.omitted` (integer, required when `sarif` is present): ≥ 1.
- **Consistency:** When both are present, `sarif.omitted` (in run.json or summary.json) MUST equal `runs[0].properties.assay.omitted_count`.
- **Normative:** SARIF upload MUST NOT fail due to size or result count; truncation is required when necessary. **Consumers MUST treat SARIF as potentially truncated and MUST use summary/run for authoritative counts.**

### 6.4 Severity Mapping

- Map Assay outcomes to SARIF severity: Fail/Error → `"error"`; Warn/Flaky → `"warning"`; Info/other → `"note"`.

---

## 7. Next-Step Requirement

For every non-zero exit, the implementation MUST provide **at least one suggested next step** so that users and CI logs know what to do next.

- **Console:** When exiting with exit_code ≠ 0, the process MUST print at least one line that is a concrete command or hint (e.g. "Run: assay doctor ...", "See: assay explain ...", "Fix baseline: assay baseline record ...").
- **summary.json:** The `next_step` field SHOULD be set when exit_code ≠ 0 (see §3.1). It MAY be the same as or a shortened form of the console message.

**Normative:** Contract tests MAY verify that for a set of known error conditions (missing config, missing trace, failing test), the output contains a non-empty next_step (in summary.json) and a console line with a suggested command.

---

## 8. Conformance

- **Producers:** `assay ci` and any code path that writes `summary.json`, `junit.xml`, or `sarif.json` for the PR gate MUST follow §2–§7.
- **Consumers:** CI workflows and the GitHub Action MAY rely on schema_version, exit_code, reason_code, and next_step as defined above. Unknown summary fields MUST be ignored.
- **Contract tests:** Implementations MUST include tests that (1) validate summary.json schema_version and required fields, (2) validate that every SARIF result has at least one location, (3) optionally validate SARIF against the official 2.1.0 schema and/or a minimal upload-smoke test.

---

## 9. Version History

| schema_version | Date     | Changes |
|----------------|----------|---------|
| 1              | 2026-01  | Initial Outputs-v1. |
| 1              | 2026-02  | Clarified/added: Seeds (§3.3.1) + Judge metrics (§3.3.2). Seeds MUST be decimal strings (or null) to avoid JSON precision loss. judge_seed reserved (null) until implemented. |
| 1              | 2026-02  | E2.3: SARIF truncation metadata (§6.3): properties.assay (truncated, omitted_count) in SARIF run; sarif.omitted in summary.json and run.json when truncated. Deterministic truncation order. |
| 1              | 2026-02  | E9c alignment draft: replay provenance keys in `provenance` (`replay`, `bundle_digest`, `replay_mode`, `source_run_id`) and `E_REPLAY_MISSING_DEPENDENCY` reason code. |
| 1              | 2026-07  | Added `E_REPLAY_LIMIT_EXCEEDED` (§5.1). Backward compatible: a new registry string under the existing `reason_code_version` 1, so consumers branching on `(reason_code_version, reason_code)` treat it as an unknown code under a known version and fall back as they already must. No version bump. |

---

## 10. References

- [ADR-019 PR Gate 2026 SOTA](./ADR-019-PR-Gate-2026-SOTA.md)
- [DX-IMPLEMENTATION-PLAN-legacy](../archive/DX-IMPLEMENTATION-PLAN-legacy.md)
- [SARIF 2.1.0](https://sarifweb.azurewebsites.net/)
- [GitHub Code Scanning SARIF](https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning)
