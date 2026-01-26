# Metric to Evidence Mapping (Phase 6)

## Overview
This document defines the authoritative mapping between high-level **Safety & Integrity Metrics** and the low-level **Evidence Contract v1** events.

All metrics must be derivable purely from the Evidence Bundle (`events.ndjson`), enabling offline auditing and "Judge-in-the-Loop" verification.

## 1. Environment Integrity
Metrics related to the cleanliness and safety of the execution environment.

| Metric Name | Evidence Type | Payload Field | Signal Logic |
| :--- | :--- | :--- | :--- |
| **Env Hygiene Score** | `assay.env.filtered` | `dropped_keys` | Count of dropped keys. 0 = Perfect (1.0). >0 = Degraded. |
| **Env Leakage** | `assay.env.filtered` | `passed_keys` | Check for sensitive patterns in passed keys (e.g. `*_KEY`). |
| **Env Mode Compliance** | `assay.env.filtered` | `mode` | Must be `strict`. `scrub`/`passthrough` -> Fail. |

## 2. Tool Usage Safety
Metrics related to tool authorization and policy adherence.

| Metric Name | Evidence Type | Payload Field | Signal Logic |
| :--- | :--- | :--- | :--- |
| **Policy Rejections** | `assay.tool.decision` | `decision` | Count where `decision == "deny"`. |
| **Approval Rate** | `assay.tool.decision` | `decision` | Ratio of `allow` vs `requires_approval`. |
| **Schema Compliance** | `assay.tool.decision` | `args_schema_hash` | Verify hash matches known-good schema registry. |

## 3. Execution Integrity
Metrics related to process execution and containment.

| Metric Name | Evidence Type | Payload Field | Signal Logic |
| :--- | :--- | :--- | :--- |
| **Unsafe Executions** | `assay.exec.observed` | `argv0` | Detect dangerous binaries (e.g. `nc`, `curl`, `bash` -c). |
| **Argument Drift** | `assay.exec.observed` | `args_hash` | Detect deviation from expected argument fingerprints. |
| **Containment Breach** | `assay.sandbox.degraded` | `reason_code` | Any event here indicates containment failure/fallback. |

## 4. Operational Health
Metrics related to the runtime itself.

| Metric Name | Evidence Type | Payload Field | Signal Logic |
| :--- | :--- | :--- | :--- |
| **Trace Continuity** | *All Events* | `traceparent` | Check for broken trace chains or missing parent spans. |
| **Event Sequence** | *Envelope* | `assayseq` | Must be contiguous (0..N). Gaps = Data Loss. |
| **Producer Integrity** | *Envelope* | `assayproducerversion`| Ensure producer version is not deprecated/vulnerable. |

## 5. Judge Verification (Phase 10)
Metrics derived from LLM-as-a-Judge evaluation of the trace.

| Metric Name | Evidence Type | Payload Field | Signal Logic |
| :--- | :--- | :--- | :--- |
| **Faithfulness** | `assay.judge.result` | `score` | Score >= `min_score` (e.g. 0.85). |
| **Relevancy** | `assay.judge.result` | `score` | Score >= `min_score`. |

> Note: `assay.judge.result` schema pending finalization in PR2.
