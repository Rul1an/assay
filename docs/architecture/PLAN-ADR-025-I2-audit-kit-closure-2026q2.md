# PLAN — ADR-025 I2 Audit Kit & Closure (2026q2)

## Intent
Iteration 2 operationalizes "Audit Kit & Closure" as evidence:
- Manifest extensions (packs applied + mapping/provenance)
- Completeness (required vs captured, signal gaps)
- Closure Score (hermetic readiness score 0.0–1.0)

Out of scope for I2:
- OTel bridge (deferred to I3)

## Inputs (frozen for Step1)
### Required
- Soak report v1 (from `assay sim soak`) — JSON
- Nightly readiness v1 (from Step3 C2) — `nightly_readiness.json`
- Evidence manifest / bundle metadata (Audit Kit surface)
  - `x-assay.packs_applied` (pack ids + versions)
  - mappings/provenance references (what mappings were applied and from where)

### Optional
- Additional evidence bundle metadata (attestations), if present

## Outputs (frozen for Step1)
- `closure_report_v1` JSON (schema: `schemas/closure_report_v1.schema.json`)
- `closure_report_v1` markdown summary (human readable)

## Closure score (v1)
Score range: 0.0–1.0 (deterministic)

Dimensions (initial):
1) Completeness score (required vs captured signals)
2) Provenance score (mappings/packs provenance present + consistent)
3) Consistency score (inputs mutually consistent: versions/classifier/policy)
4) Readiness score (derived from nightly readiness vs policy thresholds)

Weighting (v1):
- completeness: 0.40
- provenance: 0.20
- consistency: 0.20
- readiness: 0.20

## Completeness model (v1)
- required_signals[]: list of required signals (by id)
- captured_signals[]: list of captured signals (by id)
- gaps[]: required - captured
- completeness_ratio = captured_required / required_total

## Manifest extensions (v1)
Additions to manifest (conceptual contract):
- `x-assay.packs_applied[]`:
  - id, version, digest (optional), source (registry/local)
- `x-assay.mappings_applied[]`:
  - id, version, digest (optional), provenance_ref (optional)
- `x-assay.provenance`:
  - tool_version, policy_version, classifier_version (when relevant)

## Exit code contract (closure evaluation)
- 0: pass (meets closure policy / score threshold)
- 1: policy/closure fail (score below threshold, or hard violations)
- 2: measurement/contract fail (missing inputs, invalid schema, parse errors)

## Step2 implementation plan (preview)
- Implement closure evaluator (script first, CLI later)
- Deterministic scoring with fixtures
- Validate outputs against schema

## Step3 rollout plan (preview)
- Informational nightly closure artifact lane (no PR required-check impact)
- Release/promote may attach closure artifact as evidence (non-blocking unless explicitly enabled later)

## Step4 status (2026-02-20)
- Step4A merged on `main`: release integration contract freeze (`off|attach|warn|enforce`, default `attach`).
- Step4B merged on `main`: release-lane closure attach wiring via `scripts/ci/adr025-closure-release.sh`.
- Step4C closes the rollout loop: runbook, reviewer closure artifacts, and Step4C allowlist/invariant gate.
