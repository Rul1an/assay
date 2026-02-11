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
- Report includes `pass_rate`, `pass_all_k`, `first_failure_at`, `violations_by_rule`, and `infra_errors`.
- Optional in Iteration 2/3: "surface dimensions" like ε (semantic perturbations) and λ (fault injection profiles).

**Normative**: `pass^k` and drift are first-class outputs, not just "a test result".

### Track 2/3 — Evidence Completeness + Closure Score (audit + replay readiness)
We introduce a **Completeness Matrix** and **Closure Score v1**:
- **Completeness** = required signals (per pack) vs captured/redacted/unknown.
- **Closure score** = deterministic score + confidence label + `captured[]`/`missing[]`.

No "replay magic" promise; instead, a "closure contract" and explicit "what’s missing".

### Track 4 — Enforcement points (prevent + prove)
Assay packs remain "prove" (lint/verifier) in OSS. We design packs, however, so they can later verify "enforce" scenarios (MCP gateway) without pack authors needing a full programming language.
- **OSS**: only "prove" + evidence requirements.
- **Pro**: enforcement runtime + evidence emitter.

### Track 5 — Standard-first export (OTEL GenAI + MCP)
We support a dual-format approach:
1.  **Canonical Assay evidence bundle** (portable, audit artifact).
2.  **OTEL export/bridge** (GenAI/MCP semconv) for adoption.

We productize semconv versioning: emit/translate multi-version without schema wars.

### Track 6 — Portable verifiability via attestations + optional transparency
We evolve the Evidence Bundle towards an "attestation bundle":
- **DSSE envelope** (signed) with predicate(s): policy verdicts, closure report, mappings, digests.
- **Optional**: receipt/transparency (SCITT-like) later, primarily for Pro.

### Track 7 — Compliance packs as distribution
Open packs remain "accelerators":
- `cicd-starter`: adoption floor.
- `eu-ai-act-baseline`: record-keeping mapping.
- `soc2-baseline`: invariants (presence/integrity).
Next "reference pack" for R&D: OWASP Agentic Top 10 subset (OSS or Pro depending on scope).

## Data Contracts (Normative)

### 1) Soak Report v1
```json
{
  "mode": "soak",
  "iterations": 100,
  "seed": 42,
  "limits": { "...": "..." },
  "time_budget_secs": 60,
  "results": {
    "pass_rate": 0.97,
    "pass_all_k": false,
    "first_failure_at": 73,
    "violations_by_rule": {
      "soc2-baseline@1.0.0:invariant.decision_record": 2
    },
    "infra_errors": 1
  }
}
```

### 2) Completeness + Closure v1
```json
{
  "completeness": {
    "required": ["policy_decisions","tool_calls","tool_outputs_cache"],
    "captured": ["policy_decisions","tool_calls"],
    "redacted": [],
    "unknown": ["tool_outputs_cache"]
  },
  "closure": {
    "score": 0.66,
    "confidence": "medium",
    "captured": ["policy_decisions","tool_calls"],
    "missing": ["tool_outputs_cache","retrieval_snapshot","model_version"]
  }
}
```

### 3) Manifest additions (additive)
`manifest.json` adds:
- `x-assay.packs_applied[]`: `{name, version, digest, kind, source_url?}`
- `x-assay.mappings[]`: `{rule, framework, ref}`

### UX/DX Requirements (Feb 2026)
- **One-command first signal:** `assay evidence lint <bundle>` defaults to `cicd-starter` + next-step hint.
- **Explainability:** every finding hints `assay evidence lint --explain <RULE_ID>`.
- **Soak output:** 1-screen headline metrics + top violations; JSON via `--report`.
- **Config:** file-first (`--limits-file`), debug via `--print-config`.
- **Determinism:** seed in reports; fixed seed → reproducible aggregation.

## Rollout Plan (Research-aligned)

### Iteration 1 (MVP): pass^k + basic completeness/closure hooks
- `assay sim soak` + report v1
- Pack-provided "required signals" (minimal for closure/completeness)
- Manifest `x-assay` metadata (packs applied + mappings)

### Iteration 2: Reliability Surface dims + completeness matrix productization
- ε perturbations + λ fault profiles (selective)
- Completeness matrix + actionable gaps in `--explain`

### Iteration 3: Attestation envelope + OTEL bridge maturation
- DSSE envelope + digest linking
- OTEL GenAI/MCP export + mapping-loss report
- (Pro) transparency receipts / key mgmt

## Open-core Boundary

- **OSS:** Soak MVP, closure/completeness v1, open packs, OTEL bridge (opt-in).
- **Pro:** Signing/attestations key mgmt, enforcement gateway, transparency receipts, advanced OWASP/sector packs.

## Acceptance Criteria

### I1:
- `assay sim soak` + report v1.
- `manifest.json` `x-assay.*` additive metadata.
- "Required signals" concept in packs (minimal).

### I2:
- Completeness matrix + closure score v1 in JSON + explain guidance.
- Surface dims (ε/λ) prototype behind flag.

### I3:
- DSSE envelope (opt-in) + verification.
- OTEL export + mapping-loss report.
