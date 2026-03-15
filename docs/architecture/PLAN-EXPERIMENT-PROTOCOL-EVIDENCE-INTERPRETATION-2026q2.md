# PLAN: Protocol Evidence Interpretation Attacks (Q2 2026)

- Status: Step1 freeze (docs-only)
- Date: 2026-03-15
- Owner: Security/Evidence
- Ancestors: memory-poison-delayed-trigger-2026q2, delegation-spoofing-provenance-2026q2
- Scope: 4 attack vectors testing consumer-side trust downgrade under partial, ambiguous, or flattened protocol evidence
- Constraints:
  - no runtime pipeline changes
  - no LLM calls — deterministic structural testing only
  - no workflow changes

## 1) Why this experiment

The memory poisoning and delegation spoofing experiments both showed:
- Integrity checks block most vectors
- The hardest vector in each requires origin/provenance/trust-domain verification
- The full contract stack (Condition C) stops 100% of tested attacks

Both experiments tested the **producer** and **adapter** side. This experiment tests
the **consumer** side: what happens when a downstream reader misinterprets protocol
evidence that was correctly produced?

Core research question: **when does protocol-valid but incompletely interpreted
metadata lead to an overly optimistic trust decision?**

## 2) Overarching invariant

> A consumer must never produce a weaker classification than the canonical
> full-contract interpretation indicates, regardless of which subset of fields
> it reads or which precedence it applies.

## 3) Trust signal classification

All protocol evidence falls into one of three categories:

- **Verified**: backed by observed emission, provenance chain, or identity pinning
- **Self-reported**: declared by the producer/adapter without independent verification
- **Inferred**: derived by the consumer from partial signals or absence of markers

Consumer downgrades happen when self-reported or inferred signals are treated as verified.

## 4) Attack vectors

### Vector 1: Partial-Field Trust Read (Legacy Override)

**Realism class**: `consumer_realistic_synthetic`

A consumer ignores `consumer_read_path: ConvergedDecision` and reads `decision: Allow`
directly, even though `decision_outcome_kind: PolicyDeny` is present.

**Clean read** (canonical):
- Use `decision_outcome_kind` (PolicyDeny) — rank 2, verified signal
- `restrictiveness_rank()` returns 2

**Partial read** (attack):
- Use `decision` (Allow) — rank 1, self-reported legacy field
- `restrictiveness_rank()` returns 1

**Payload:**
```
ReplayDiffBasis {
    decision_outcome_kind: Some(PolicyDeny),
    decision: Allow,
    // remaining fields consistent with a converged deny event
}
```

**Realism note:** This combination is consumer-realistic synthetic. The existing
normalization pipeline aligns `decision` with `decision_outcome_kind` for new events,
but pre-Wave24 legacy events or partially-migrated consumers may encounter this shape
via replay or cross-version evidence.

**Success:** Consumer classifies as Allow (rank 1) when canonical says Deny (rank 2).
**Failure:** Consumer uses `decision_outcome_kind` per read-path precedence.

### Vector 2: Precedence Inversion (Deny Convergence)

**Realism class**: `producer_realistic`

A consumer reads the correct deny-related fields but applies the wrong tier precedence.
It trusts tier-4 legacy deny flags, ignoring tier-1 `decision_outcome_kind`.

**Key distinction from V3:** V2 reads multiple trust-relevant fields correctly but in
the wrong order. V3 suppresses trust signals entirely.

**Clean read** (canonical):
- `decision_outcome_kind: EnforcementDeny` via tier 1 (`OutcomeKind`) — enforcement deny

**Inverted read** (attack):
- `deny_classification_source: LegacyDecision` (tier 4), `policy_deny: false`,
  legacy `decision: Allow` — appears not deny

**Payload:**
```
ReplayDiffBasis {
    decision_outcome_kind: Some(EnforcementDeny),
    policy_deny: false,      // legacy field not yet normalized
    enforcement_deny: false,  // legacy field not yet normalized
    deny_classification_source: LegacyDecision,
    decision: Allow,
}
```

**Success:** Consumer misclassifies an enforcement deny as not-deny.
**Failure:** Consumer applies tier-1 precedence (`decision_outcome_kind` wins).

### Vector 3: Compat Flattening (Trust Signal Suppression)

**Realism class**: `consumer_realistic` (common in SDK/analytics chains)

A consumer does not read all trust-relevant fields. It treats
`ConsumerPayloadState::CompatibilityFallback` as decorative metadata rather than
a binding trust signal, and uses converged-looking fields as-is without checking
compat state.

**Key distinction from V2:** V3 does not misorder fields — it suppresses compat/trust
signals entirely, treating them as non-binding. V2 reads the right signals in the
wrong precedence.

**Clean read** (canonical):
- `consumer_payload_state: CompatibilityFallback` + `consumer_fallback_applied: true`
  triggers cautious downstream handling

**Flattened read** (attack):
- Consumer skips `consumer_payload_state`, uses converged-looking fields as-is

**Payload:**
```
ReplayDiffBasis {
    consumer_payload_state: CompatibilityFallback,
    consumer_fallback_applied: true,
    consumer_read_path: CompatibilityMarkers,
    // but converged-looking decision fields present
    decision_outcome_kind: Some(ObligationApplied),
    decision: Allow,
}
```

**Success:** Consumer treats compat-fallback payload as equivalent to fully converged,
producing `EvidenceOnly` diff when canonical says `Reclassified`.
**Failure:** Consumer checks `consumer_payload_state` and handles compat as distinct.

### Vector 4: Projection Loss (Required Fields Dropped)

**Realism class**: `adapter_realistic` (serialization/forwarding chains)

A consumer drops one or more of the 6 required consumer fields during serialization,
caching, or forwarding. The dropped field causes silent read-path fallback.

**Clean read** (canonical):
- All 6 `required_consumer_fields` present — `ConvergedDecision` path (verified)

**Lossy read** (attack):
- All converged fields (`decision_outcome_kind`, `decision_origin`,
  `fulfillment_decision_path`) and all compat markers (`decision_basis_version`,
  `compat_fallback_applied`, `classification_source`, `legacy_shape_detected`)
  dropped in transit — only legacy `decision` survives

**Payload:**
```
// Full basis serialized, then all converged + compat fields stripped
ReplayDiffBasis {
    decision_outcome_kind: None,
    decision_origin: None,
    fulfillment_decision_path: None,
    decision_basis_version: "",           // empty / absent
    compat_fallback_applied: false,
    classification_source: LegacyFallback,
    legacy_shape_detected: false,
    // only legacy field survives:
    decision: Allow,
}
```

This simulates a forwarding chain (SDK, cache, analytics pipeline) that strips
all Wave24+ fields and preserves only the legacy `decision` field.

**Success:** Consumer silently falls from `ConvergedDecision` to `LegacyDecision`
read path, producing a different classification.
**Failure:** Consumer validates required-field completeness before classification.

## 5) Benign controls

- **Control E1: Legitimate legacy payload** — A genuinely old payload with only `decision`
  field. Consumer correctly uses `LegacyDecision`. Must not be flagged.
- **Control E2: Legitimate compat fallback** — A payload from a pre-Wave41 producer with
  partial converged fields. Consumer correctly uses `CompatibilityMarkers`. Must not be flagged.
- **Control E3: Legitimate full-converged read** — A complete payload correctly read via
  `ConvergedDecision`. Must not be flagged.

## 6) Experimental conditions

### Condition A: Unprotected

Consumer reads `decision` directly, ignores `consumer_read_path` and all convergence
signals. Treats all signals as self-reported.

### Condition B: Precedence-aware, trust-incomplete

Consumer respects `consumer_read_path` precedence and uses converged fields when
available. Follows canonical field ordering. But treats compat/completeness signals
as non-binding metadata — does not enforce `consumer_payload_state` or
`required_consumer_fields`.

### Condition C: Full consumer hardening

Condition B, plus:
- `consumer_payload_state` treated as binding (compat != converged)
- `required_consumer_fields` completeness enforced (missing = fallback)
- `deny_classification_source` precedence enforced (tier 1 wins)
- Verified/self-reported/inferred distinction maintained throughout

## 7) Metrics

### Primary (per-vector)

- **Consumer Downgrade Rate (CDR)**: fraction of payloads where consumer produces a
  weaker classification than the canonical converged-contract interpretation
- **Precedence Inversion Rate (PIR)**: fraction of deny classifications where the
  consumer reads the wrong deny tier
- **Compat Flattening Rate (CFR)**: fraction of compat-fallback payloads treated as
  converged
- **Projection Loss Rate (PLR)**: fraction of field-dropped payloads that silently
  change read path

### Global

- **Canonical Consumer Agreement Rate (CCAR)**: fraction of all payloads (across all
  vectors and conditions) where the consumer outcome matches the canonical full-contract
  interpretation. Enables cross-vector and cross-experiment comparison.
- **FPBR**: fraction of benign controls incorrectly flagged

## 8) Hypotheses

- **H1:** Under Condition B, CDR and PIR drop below 10% (read-path precedence catches
  V1 and V2)
- **H2:** Under Condition C, all rates drop below 5% (compat checking + required-field
  completeness catch V3 and V4)
- **H3:** FPBR stays below 2%
- **H4 (falsifiable):** V3 (compat flattening) has the highest CFR under Condition B,
  because `same_effective_decision_class()` compares `consumer_payload_state` but a
  consumer reading before that function does not

## 9) Result output shape

```json
{
  "vector_id": "v1_partial_trust_read",
  "condition": "condition_c",
  "realism_class": "consumer_realistic_synthetic",
  "canonical_classification": "deny",
  "consumer_classification": "deny",
  "downgrade_occurred": false,
  "outcome": "no_effect",
  "hypothesis_tags": ["H1"]
}
```

### Success taxonomy

- **no_effect**: consumer and canonical agree
- **retained_no_downgrade**: consumer diverges in metadata but not in classification
- **downgrade_with_correct_detection**: downgrade attempted but invariant caught it
- **silent_downgrade**: consumer produces weaker classification without detection
- **silent_trust_upgrade**: consumer treats self-reported/inferred as verified

## 10) Wave structure

### Step 1 (this freeze): Docs + gate only

- This plan document
- `docs/contributing/SPLIT-PLAN-experiment-protocol-evidence-interpretation.md`
- `docs/contributing/SPLIT-CHECKLIST-experiment-protocol-evidence-interpretation-step1.md`
- `scripts/ci/review-experiment-protocol-evidence-step1.sh`

Frozen: all `crates/`, all `.github/workflows/`

### Step 2: Implementation

- `crates/assay-sim/src/attacks/consumer_downgrade.rs`
- `crates/assay-sim/tests/consumer_downgrade_invariant.rs`

Must not touch: `crates/assay-core/src/mcp/decision.rs`, `.github/workflows/`

### Step 3: Closure

- Results analysis with CCAR cross-experiment comparison
- Hypothesis validation (H1-H4)
- Cross-experiment synthesis (trifecta: producer / adapter / consumer)

## 11) Explicit non-goals

- No runtime decision pipeline changes
- No LLM-based detection
- No workflow changes
- No new consumer implementation (tests simulate consumer behavior)
- No external identity/auth provider

## 12) Relationship to prior experiments

This experiment completes the trust-chain trifecta:

- **Memory poisoning** (producer-side): field provenance validation is decisive
- **Delegation spoofing** (adapter-side): trust-domain verification is decisive
- **Protocol evidence interpretation** (consumer-side): this experiment

Together they test the full trust chain from state injection through protocol
translation to evidence interpretation.
