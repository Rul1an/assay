# PLAN: ADR-025 Iteration 1 (Q2 2026) — Audit Kit + Soak MVP

- Status: Active (next execution point)
- Date: 2026-02-17
- Owner: Evidence/DX + CI Hardening
- Scope: `assay sim soak` + `soak-report-v1` + minimal audit-kit provenance links + non-blocking readiness reporting
- Constraints:
  - no required PR-check expansion in I1
  - no dashboard/managed-store scope in I1
  - deterministic outputs and stable contracts over breadth

## 1) Why this is next

Highest-value P1 open items:

1. Audit Kit (manifest/provenance linkage)
2. Soak testing + pass^k reliability surface

Codebase verification snapshot:

- `crates/assay-cli/src/cli/args/mod.rs`: `SimSub` currently exposes `Run(...)`; no `Soak(...)`.
- `crates/assay-cli/src/cli/commands/sim.rs`: command dispatcher currently handles `SimSub::Run` only.
- `.github/workflows/release.yml`: attestation produce + verify loop exists and is reusable as I1 baseline.

## 2) External alignment (Feb 2026)

I1 is shaped around contemporary reliability and governance practice:

1. Multi-run reliability with confidence intervals and deterministic seeds.
2. Anti-gaming posture with required canary checks.
3. Separation of policy failures from measurement/infra failures.
4. Supply-chain verifiability through attestation produce+verify loops.
5. Staged rollout: informational lane first, fail-closed enforcement only in release/promote paths.

## 3) Scope and non-goals

### In scope (I1)

1. New CLI contract: `assay sim soak`.
2. Strict report contract: `soak-report-v1`.
3. Deterministic seeded execution.
4. Audit-kit provenance linkage fields.
5. Informational nightly readiness reporting and promotion evidence artifact.

### Out of scope (I1)

1. Full completeness/closure scoring engine (I2).
2. Managed dashboard/store.
3. Branch-protection rewiring or new required PR checks.

## 4) Normative contracts

### 4.1 CLI

```bash
assay sim soak --iterations <N> --seed <u64> --target <bundle.tar.gz> --report <path>
```

Exit contract:

1. `0`: pass under configured decision policy.
2. `1`: policy threshold violation.
3. `2`: measurement/infra failure (including schema/contract validation failure).

### 4.2 Evaluation unit

Primary I1 evaluation unit is `scenario` (each scenario execution is one iteration row).

### 4.3 Report contract

`schemas/soak_report_v1.schema.json` is the source-of-truth schema.

Required top-level sections:

1. `run` (seed/iterations/budget/evaluation unit)
2. `target` (bundle + packs + digests)
3. `decision_policy`
4. `aggregate`:
   - `pass_all`
   - `pass_rate`
   - `ci_95`
   - `dimensions` (correctness/safety/security/control)
   - `violations_by_rule`
   - `canaries` (required)

### 4.4 Audit-kit linkage (I1 baseline)

Minimum manifest/provenance linkage in I1:

1. `x-assay.packs_applied`
2. `x-assay.mappings`
3. soak report digest reference (manifest-linked)

## 5) A/B/C PR slicing

### PR-A (Step1 freeze)

Docs/schema/gates only:

1. schema freeze (`soak-report-v1`)
2. inventory/checklist/review-pack
3. reviewer script with allowlist + contract anchors

### PR-B (Step2 implementation)

Code implementation:

1. `SimSub::Soak` args + dispatch
2. report generation + strict schema validation
3. deterministic/statistical tests

### PR-C (Step3 rollout)

Workflow/gating rollout:

1. nightly informational lane
2. release/promote fail-closed verify path
3. env switch: `ASSAY_SOAK_GATE=off|warn|enforce`

## 6) Promotion criteria (I1)

Promotion-ready is true only if all are true:

1. Data sufficiency:
   - most recent 20 scheduled runs on main
   - minimum 14 runs and minimum 14-day span
2. Stability:
   - pass rate (excluding infra category) >= 0.95
3. Uncertainty:
   - CI95 lower bound >= threshold policy floor
4. Flake budget:
   - `flake_rate <= 0.05` (I1 deterministic flake rule)
5. Duration budget:
   - median <= 20m and p95 <= 35m
6. Governance safety:
   - no required PR-check changes introduced

## 7) Risks and mitigations

1. Eval gaming risk:
   - enforce canary checks and track canary drift.
2. False confidence from retries:
   - explicit flake taxonomy and separate infra category.
3. CI blast radius:
   - informational first; fail-closed only in release/promote lanes.
4. Contract drift:
   - strict schema + reviewer script + frozen allowlist.

## 8) Definition of done (I1)

I1 is done when:

1. `assay sim soak` is available and deterministic by seed.
2. generated report validates `soak-report-v1` schema.
3. aggregate includes ARES-like dimensions: correctness/safety/security/control.
4. canary checks are required and emitted.
5. readiness report artifact exists and promotion decision is reproducible.

## 9) Interop spine freeze (Step1)

I1 interoperability baseline is fixed as:

1. CloudEvents-compatible event envelope discipline.
2. W3C Trace Context correlation fields in soak output (`traceparent`, `tracestate`).
3. Replay/correlation identifiers kept as first-class report metadata.

## 10) Provenance and attestation freeze (Step1)

Audit-kit provenance baseline for I1:

1. DSSE envelope as signing/verification container.
2. in-toto statement model for predicate separation.
3. SLSA provenance v1 predicate as baseline provenance payload.

Assay-specific claims stay additive as extra predicates; no custom signing format is introduced in I1.

## 11) OTel GenAI alignment freeze (Step1)

I1 adopts OTel GenAI vocabulary where it is stable and practical:

1. span/metric-aligned field naming preferred over custom terms.
2. events remain optional in I1 output shape.
3. mapping loss is explicitly tolerated in I1 and tracked for I2/I3 bridge hardening.

## 12) Soak methodology freeze (Step1)

Variance-aware methodology is mandatory in I1:

1. multi-trial output (`trials[]`) with per-trial seed/outcome/timing.
2. aggregate `summary` with confidence interval and thresholds used.
3. ARES-like dimension accounting: correctness/safety/security/control.
4. anti-gaming canaries are first-class signals (`canaries[]`), not optional notes.

## 13) SARIF constraints freeze (Step1)

Soak-to-SARIF mapping constraints (for later rollout steps):

1. do not upload unsupported multi-run SARIF combinations as one logical upload.
2. use one run per upload or enforce unique run lineage/category strategy.
3. keep trial-level detail in soak artifacts; publish SARIF as policy-ready projection.

## 14) Schema strategy freeze (Step1)

`soak_report_v1` schema policy:

1. JSON Schema draft 2020-12 (`$schema`) is fixed baseline.
2. explicit `report_version` is required for contract evolution.
3. strict top-level shape (`additionalProperties: false`) with deliberate extensibility only in selected nested maps (e.g., thresholds).
4. v1 -> v2 migration remains additive-first; removals require explicit contract phase and fixtures.
