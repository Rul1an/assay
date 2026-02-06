# Run Output Contract (PR Gate)

> **Purpose**: Machine-readable outputs from `assay run` and `assay ci` for CI gates and downstream tooling.
> **Version**: 2.15.0 (February 2026)
> **Spec**: [SPEC-PR-Gate-Outputs-v1](../architecture/SPEC-PR-Gate-Outputs-v1.md) — §3.3.1 Seeds, §3.3.2 Judge metrics, §6.3 SARIF truncation.
> **Implementation**: PR #159 (E7.5, E7.2, E7.3); PR #160 (E2.3 SARIF limits, sarif.omitted).

## Overview

After a run, Assay writes:

1. **run.json** — Exit outcome, reason code, seeds, and judge metrics (extended or minimal on early-exit).
2. **summary.json** — Full `Summary` with schema_version, reason_code_version, seeds, judge_metrics, results, performance.
3. **Console footer** (stderr) — One line: `Seeds: seed_version=1 order_seed=… judge_seed=…`; then a judge metrics line when present.

Consumers should branch on **`(reason_code_version, reason_code)`** for semantics; exit code is coarse transport only.

## run.json

| Field | Type | Description |
|-------|------|-------------|
| `exit_code` | integer | 0 = success, 1 = test/judge failure, 2 = config, 3 = infra |
| `reason_code` | string | e.g. `E_TEST_FAILED`, `E_JUDGE_UNCERTAIN`, `E_TRACE_NOT_FOUND` |
| `reason_code_version` | integer | MUST be `1` for Outputs-v1 |
| `seed_version` | integer | MUST be `1`; present even on early-exit |
| `order_seed` | string \| null | Decimal u64 as string, or null when unknown (e.g. early-exit) |
| `judge_seed` | string \| null | Decimal u64 as string, or null (reserved until E9) |
| `judge_metrics` | object \| absent | Optional; see below |
| `sarif` | object \| absent | Present when SARIF was truncated (PR #160): `{ "omitted": N }` |

Seeds are **strings or null** (not JSON numbers) to avoid precision loss in JS/TS (u64 > 2^53).

## summary.json

Contains all run.json outcome fields plus:

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | integer | Summary schema version |
| `reason_code_version` | integer | Reason code registry version |
| `seeds` | object | Required; `{ "seed_version": 1, "order_seed": string|null, "judge_seed": string|null }` |
| `judge_metrics` | object \| null | Optional; present when judge was used |
| `sarif` | object \| null | Optional; present when SARIF was truncated (PR #160): `{ "omitted": N }` |

## Replay provenance (E9c alignment draft)

For replay-generated outputs (`assay replay --bundle ...`), `summary.json` / `run.json` provenance is aligned to carry:

| Field | Type | Description |
|-------|------|-------------|
| `provenance.replay` | boolean | `true` when output came from replay bundle execution |
| `provenance.bundle_digest` | string | SHA256 digest of the bundle archive |
| `provenance.replay_mode` | string | `"offline"` or `"live"` |
| `provenance.source_run_id` | string \| absent | Optional original run identifier |

Replay offline contract uses reason code **`E_REPLAY_MISSING_DEPENDENCY`** (exit code 2) when required inputs are missing.

## judge_metrics (when present)

| Field | Type | Description |
|-------|------|-------------|
| `abstain_rate` | number | Fraction of evaluations that were abstain/uncertain |
| `flip_rate` | number | Heuristic: rate of verdict flips (e.g. A/B vs B/A) |
| `consensus_rate` | number | Fraction where all iterations agreed |
| `margin` | number | Average distance to decision boundary |

Low-cardinality only; no per-trace or high-cardinality labels.

## Console footer

- **Seeds line**: `Seeds: seed_version=1 order_seed=<value> judge_seed=<value>`
  Values are the decimal string or `null`. Normative format per SPEC §3.3.1.
- **Judge metrics line**: Printed when judge_metrics is present (e.g. abstain_rate, flip_rate).

## Reason codes (exit 1)

| reason_code | Meaning |
|-------------|---------|
| `E_TEST_FAILED` | One or more tests failed (assertion/metric) |
| `E_JUDGE_UNCERTAIN` | Judge returned abstain; cannot decide pass/fail (PR #159) |
| `E_POLICY_VIOLATION` | Policy rule violated |

## sarif.omitted (PR #160)

When **SARIF was truncated** (e.g. more than 25k eligible results), both run.json and summary.json include a top-level **`sarif`** object with **`omitted`** (integer ≥ 1). When no truncation occurred, the `sarif` key is **absent**. Consumers MUST use summary/run for authoritative result counts when `sarif.omitted` is present; the SARIF file itself is truncated and has `runs[0].properties.assay.truncated` and `omitted_count`. See SPEC §6.3 and [Architecture Diagrams](architecture-diagrams.md#sarif-truncation-flow-pr-160).

## Early-exit behavior

On config error, missing trace, or similar early-exit:

- **run.json** (minimal): exit_code, reason_code, reason_code_version, seed_version present; **order_seed** and **judge_seed** may be **null**.
- **summary.json**: Same; seeds object present with seed_version; order_seed/judge_seed null when unknown.

## Related

- [Quick Reference](quick-reference.md) — Exit codes and Run/CI output table
- [Architecture Diagrams](architecture-diagrams.md) — Run Output (PR Gate), SARIF Truncation Flow (PR #160)
- [SPEC-PR-Gate-Outputs-v1](../architecture/SPEC-PR-Gate-Outputs-v1.md) §6.3
