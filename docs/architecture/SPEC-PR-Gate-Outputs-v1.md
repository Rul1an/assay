# PR Gate Output Contracts Specification v1

**Status:** Draft
**Version:** 1.0.0
**Date:** 2026-01
**ADR:** [ADR-019: PR Gate 2026 SOTA](./ADR-019-PR-Gate-2026-SOTA.md)
**Related:** [DX-IMPLEMENTATION-PLAN](../DX-IMPLEMENTATION-PLAN.md), [SPEC-GitHub-Action-v2.1](./SPEC-GitHub-Action-v2.1.md)

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
| `reason_code`         | string  | **Yes**  | Stable machine-readable code when exit_code ≠ 0; e.g. `E_TRACE_NOT_FOUND`, `E_JUDGE_UNAVAILABLE`. See §5. MAY be empty string when exit_code is 0. |
| `message`             | string  | No       | Human-readable one-line description of outcome. |
| `next_step`           | string  | No       | Single suggested command or hint when exit_code ≠ 0; e.g. "Run: assay doctor --config ...", "See: assay explain ...". See §7. |

### 3.2 Provenance (Artifact Auditability)

Every summary.json MUST include the following provenance fields so that gates remain auditable (ADR-019 P0.4).

| Field                 | Type   | Required | Description |
|-----------------------|--------|----------|-------------|
| `assay_version`       | string | **Yes**  | Assay CLI version that produced this run (e.g. `"2.12.0"`). |
| `verify_mode`         | string | **Yes**  | `"enabled"` or `"disabled"`. When `"disabled"`, indicates signature verification was turned off (UNSAFE). |
| `policy_pack_digest`  | string | No       | Digest of policy/pack used (e.g. `sha256:...`). |
| `baseline_digest`     | string | No       | Digest of baseline used for comparison, if applicable. |
| `trace_digest`        | string | No       | Digest of trace input, if applicable (optional for privacy/size). |

**Normative:** If the run was executed with `--no-verify`, `verify_mode` MUST be `"disabled"`.

### 3.3 Results Summary (Optional but Recommended)

| Field           | Type   | Required | Description |
|-----------------|--------|----------|-------------|
| `passed`        | integer| No       | Count of tests passed. |
| `failed`        | integer| No       | Count of tests failed. |
| `warned`        | integer| No       | Count of tests with Warn/Flaky (depends on strict mode). |
| `skipped`       | integer| No       | Count of tests skipped (e.g. cache hit). |
| `total_duration_ms` | integer | No | Total run duration in milliseconds. |

Future versions of this schema MAY add `slowest_tests`, `cache_hit_rate`, `phase_timings` (see ADR-019 / DX-IMPLEMENTATION-PLAN). Consumers MUST ignore unknown top-level keys.

### 3.4 Example (Minimal)

```json
{
  "schema_version": 1,
  "reason_code_version": 1,
  "exit_code": 0,
  "reason_code": "",
  "assay_version": "2.12.0",
  "verify_mode": "enabled",
  "passed": 10,
  "failed": 0,
  "total_duration_ms": 1234
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
  "assay_version": "2.12.0",
  "verify_mode": "enabled"
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

### 5.2 Infra / Judge Unavailable (exit_code 3)

| Code                | Description |
|---------------------|-------------|
| E_JUDGE_UNAVAILABLE | Judge service unavailable or returned error. |
| E_RATE_LIMIT        | Judge/provider rate limit hit. |
| E_PROVIDER_5XX      | Judge/provider returned 5xx. |
| E_TIMEOUT           | Judge or dependency timed out. |

### 5.3 Test Failure (exit_code 1)

Test-level failures MAY use existing policy/metric codes (e.g. E_ARG_SCHEMA, E_SEQUENCE_VIOLATION) or a generic E_TEST_FAILED. The summary.json reason_code for the run MAY be E_TEST_FAILED when at least one test failed and no single dominant reason is reported.

**Normative:** When exit_code ≠ 0, summary.json MUST set `reason_code` to one of the registered values (or a documented extension). Implementations MUST NOT leave reason_code empty when exit_code ≠ 0.

---

## 6. SARIF Contract (GitHub Compatibility)

SARIF produced for GitHub Code Scanning MUST satisfy the following so that `upload-sarif` does not reject the file.

### 6.1 Schema and Version

- SARIF version MUST be `"2.1.0"`.
- Schema URI MUST be the official SARIF 2.1.0 JSON schema.

### 6.2 Location Requirement

- **Every result MUST have at least one location.** If no file/line is available, the producer MUST emit a **synthetic location** (e.g. URI `assay.yaml`, `policy.yaml`, or the config path). GitHub's upload can fail with "expected at least one location" when a result has an empty `locations` array.

**Normative:** Contract tests MUST validate that every result in the generated SARIF has `locations` length ≥ 1.

### 6.3 Truncation (Size and Result Limits)

- GitHub enforces limits on SARIF upload (e.g. max size gzipped, max number of results). Producers MUST **truncate** results when limits would be exceeded, and MUST add a clear indication that results were omitted (e.g. in the run description or a dedicated message: "N results omitted due to GitHub upload limits").
- Truncation strategy: keep top N results by severity (e.g. error first, then warning). N and the exact message are implementation-defined but MUST be documented.
- **Normative:** SARIF upload MUST NOT fail due to size or result count; truncation is required when necessary.

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
| 1              | 2026-01  | Initial: schema_version, exit_code, reason_code, provenance, next_step, SARIF location and truncation rules. |

---

## 10. References

- [ADR-019 PR Gate 2026 SOTA](./ADR-019-PR-Gate-2026-SOTA.md)
- [DX-IMPLEMENTATION-PLAN](../DX-IMPLEMENTATION-PLAN.md)
- [SARIF 2.1.0](https://sarifweb.azurewebsites.net/)
- [GitHub Code Scanning SARIF](https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning)
