# ADR-025: Evidence-as-a-Product — Reliability Surfaces, Completeness/Closure, and Portable Verifiability

## Status
Proposed (Feb 2026)

## Context
The agent engineering market (2026) has commoditized "eval CI" and "observability". Differentiation now comes from:

1.  **Multi-run reliability** as a first-order property (pass^k, stress/fault surfaces).
2.  **Auditability** as a sociotechnical system (tamper-evident + governance context, not just logs).
3.  **Security** via enforcement points (policy enforcement + evidence emission).
4.  **Standard-first interoperability** (OTEL GenAI + MCP semconv).
5.  **Attestation/transparency stacks** as evidence substrate (in-toto/DSSE/SCITT/Sigstore/SLSA).
6.  **Compliance hooks** (EU AI Act Art 12/19, OWASP Agentic Top 10).
7.  **CI gates** as a commodity integration surface (Actions/Evals), not a differentiator.

Assay’s wedge is **portable, verifiable “evidence primitives”** + **policy packs** + **stability assurance** (pass^k) + **closure/confidence**.

## Decision

### Track 1 — Reliability Surface (pass^k + faults) as Evidence
We introduce **Soak/Surface** as the primary simulation product:
- `assay sim soak` executes N runs (seeded), collecting policy outcomes, infra errors, and summary metrics.
- **Pass^k Semantics**: `pass_all` (AND over k runs) is the strict assurance bar. We also report `pass_rate` and `pass_probability_estimate` (beta posterior or CI95) for statistical confidence.
- **Decision Policy**: User defines strictness via `decision_policy`: `{ stop_on_violation: bool, max_failures: u32, min_runs: u32 }`.

**Normative**: `pass^k` and drift are first-class outputs. Soak reports must include a `decision_policy` used to reach the verdict.

### Track 2/3 — Evidence Completeness + Closure Score (audit + replay readiness)
We distinguish between **Completeness** (Pack-Relative) and **Closure** (Replay-Relative):

1.  **Completeness** ("Did we capture what the Pack needs?"):
    -   Defined relative to a *specific pack*.
    -   Signals are defined in a canonical registry (e.g., `policy_decisions`, `tool_calls`).
    -   State: `captured` (present), `redacted` (removed but committed with hash/metadata), `unknown` (missing/undetectable).

2.  **Closure** ("Can we reconstruct the run?"):
    -   Defined relative to *replayability*.
    -   Score is deterministic (0.0-1.0) based on presence of replay-critical signals (inputs, model ID, tool outputs, RNG seeds).
    -   **Normative**: Score must be calculated from the bundle contents alone (no heuristics).

### Track 4 — Pack "Required Signals" Registry
To prevent schema drift, we introduce a namespaced, additive field for packs:
-   **Field**: `x-assay.requires_signals` (v0).
-   **Registry**: Packs must select from a canonical list of signal types (e.g., `policy_decisions`, `tool_io_bodies`, `model_identity`, `prompt_lineage`, `human_approvals`).
-   This avoids free-text requirements and enables automated completeness checks.

### Track 5 — Standard-first export (OTEL GenAI + MCP)
We support a dual-format approach with a strict versioning policy:
1.  **Target**: GenAI semconv (stable opt-in) + MCP semconv.
2.  **Policy**: Best-effort translation.
3.  **Transparency**: Reports must include a `mapping_loss` section detailing dropped attributes and unknown events.

### Track 6 — Attestation Envelope (Portable Verifiability)
We evolve the Evidence Bundle towards an "attestation bundle" using **DSSE envelopes**.
-   **Payload v1**:
    -   Digests of bundle artifacts (manifest, events).
    -   Pack versions + mapping references.
    -   Closure report digest.
-   **Verification**: OSS capability for offline verification (providing public key).
-   **Threat Model**: Protects against tampering (integrity) and repudiation (provenance). Does *not* guarantee confidentiality (payload content).

## Data Contracts (Normative)

### 1) Soak Report v1 (Normative)
```json
{
  "schema_version": "v1",
  "mode": "soak",
  "config": {
    "iterations": 100,
    "seed": 42,
    "limits": { "max_duration": "60s" },
    "decision_policy": {
      "stop_on_violation": false,
      "max_failures": 0,
      "min_runs": 100
    }
  },
  "results": {
    "verdict": "pass",
    "pass_all": false,
    "pass_rate": 0.97,
    "pass_rate_ci95": [0.92, 0.99],
    "first_failure_at": 73,
    "failures": 3,
    "runs": 100,
    "violations_by_rule": {
      "soc2-baseline@1.0.0:invariant.decision_record": 2
    },
    "infra_errors": 1
  }
}
```

### 2) Completeness + Closure v1 (Normative)
```json
{
  "schema_version": "v1",
  "completeness": {
    "pack_scope": "soc2-baseline@1.0.0",
    "required": ["policy_decisions", "tool_calls"],
    "captured": ["policy_decisions"],
    "redacted": [],
    "unknown": ["tool_calls"]
  },
  "closure": {
    "score": 0.66,
    "confidence": "medium",
    "captured": ["policy_decisions", "tool_calls"],
    "missing": ["tool_outputs_cache", "model_identity"]
  }
}
```

### 3) Manifest additions
`manifest.json` attributes:
- `x-assay.packs_applied[]`: `{name, version, digest, kind, source_url?}`
- `x-assay.mappings[]`: `{rule, framework, ref}`

### UX/DX Requirements (Feb 2026)
- **Unified Happy Path:**
    - `assay evidence lint <bundle>` (default: lint with `cicd-starter`).
    - `assay sim soak --iterations N --pack <pack> --target <bundle> --report out.json`.
    - **Normative**: Soak must use the *same* pack loader/resolution as Lint.
- **Explainability:**
    - `assay evidence lint --explain closure.score`.
    - `assay evidence lint --explain <pack>:<rule>` (shows missing signals if applicable).
- **Machine-Readable Reports:** ALL commands supporting `--report` must output JSON with `schema_version`. Stdout remains human-readable summary.

## Rollout Plan

### Iteration 1 (MVP): Audit Kit Baseline & Soak MVP
- `assay sim soak` + report v1 (with `decision_policy`, `pass_rate`).
- `manifest.json` `x-assay.*` metadata.
- Pack-provided `x-assay.requires_signals` (minimal registry).

### Iteration 2: Closure Score & Explainability
- Completeness Matrix (Pack-relative) + Closure Score (Replay-relative).
- `redacted` vs `unknown` definitions in lint reporting.
- Advanced `--explain` (closure gaps).

### Iteration 3: Attestation & OTEL
- DSSE envelope generation (opt-in) + offline verify command.
- OTEL export + `mapping_loss` report.

## Open-core Boundary

- **OSS:** Soak MVP, Closure/Completeness v1, Open Packs, OTEL bridge (opt-in), Offline Verification.
- **Pro:** Signing/Attestation Key Mgmt, Enforcement Gateway, Private/Advanced Packs.

## Acceptance Criteria (Gates)

### I1:
- `assay sim soak` with `report.json` containing `schema_version` and valid `pass_rate`/`pass_all`.
- Manifest `x-assay` fields populated correctly.
- Packs in OSS repo define `requires_signals` from v0 registry; parser validates this.

### I2:
- Completeness matrix calculation is deterministic.
- Closure score logic documented and tested with fixed fixtures.
- `--explain` renders closure gaps.

### I3:
- DSSE envelope generation (feature-flagged).
- OTEL export produces valid SemConv + `mapping_loss` section in report.
