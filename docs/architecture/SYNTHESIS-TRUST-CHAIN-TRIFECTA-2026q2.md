# Trust-Chain Trifecta: Experiment Synthesis (Q2 2026)

## Status

All three experiments complete. This document synthesizes the cross-experiment findings.

## The Three Experiments

| Experiment | Perspective | What it tests |
|-----------|------------|--------------|
| [Memory Poisoning](RESULTS-EXPERIMENT-MEMORY-POISON-2026q2.md) | Producer-side | Can poisoned state entries shift classification later? |
| [Delegation Spoofing](RESULTS-EXPERIMENT-DELEGATION-SPOOFING-2026q2.md) | Adapter-side | Can capability/provenance/identity claims deceive consumers? |
| [Protocol Evidence Interpretation](RESULTS-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md) | Consumer-side | Can incomplete reads silently downgrade trust decisions? |

## Consolidated Results

| Experiment | A (unprotected) | B (integrity/precedence) | C (full stack) | FPBR |
|-----------|----------------|------------------------|---------------|------|
| Memory Poisoning | DASR 100% | DASR 25% | **DASR 0%** | 0% |
| Delegation Spoofing | 4/4 bypass | 1/4 bypass | **0/4 bypass** | 0% |
| Protocol Evidence | CCAR 0% | CCAR 50% | **CCAR 100%** | 0% |

Each experiment uses the primary metric most appropriate to its perspective:
activation rate for producer-side, bypass count for adapter-side, and canonical
consumer agreement for consumer-side.

## The Pattern

All three experiments independently confirm the same structural principle:

> **Integrity/precedence (Condition B) handles most attacks. The hardest attack in
> each experiment requires a deeper verification layer (Condition C). Zero false
> positives across all experiments and conditions.**

### What B blocks

| Experiment | Vectors blocked by B | Mechanism |
|-----------|---------------------|-----------|
| Memory Poisoning | V1, V2, V4 | Content-addressed hash comparison |
| Delegation Spoofing | V1, V2, V4 | Claim-vs-emission, provenance tracking |
| Protocol Evidence | V1, V3 | Read-path precedence |

### What only C blocks

| Experiment | Hardest vector(s) | C-layer mechanism |
|-----------|------------------|------------------|
| Memory Poisoning | V3 (context envelope) | Field provenance validation |
| Delegation Spoofing | V3 (identity spoofing) | Trust-domain verification |
| Protocol Evidence | V2 + V4 (precedence + projection) | Tier-1 enforcement + completeness validation |

### The meta-lesson

The hardest attacks are never broken data. They are:
- **Schema-valid**: pass all structural checks
- **Protocol-plausible**: could realistically occur in production
- **Semantically misleading**: shift interpretation through ambiguity, not corruption

Hashes, integrity checks, and read-path awareness catch most threats.
But the decisive defense against the hardest attacks is always about
**interpreting origin, provenance, and completeness correctly** — not
just checking that bytes are intact.

## Design Principles (derived from all three experiments)

### 1. Field provenance is a security control

A field's presence must be validated against when it was produced, not just
whether it passes schema validation. (Memory Poisoning V3)

### 2. Trust-domain verification must go beyond metadata

Source URN, protocol name, and capability claims are insufficient for trust.
Adapter identity must be pinned or signed. (Delegation Spoofing V3)

### 3. Consumer read precedence must be binding

`consumer_read_path` and `required_consumer_fields` are not optional metadata.
They are security-critical contract fields that determine classification
correctness. (Protocol Evidence V2, V4)

### 4. Compat signals are trust signals

`ConsumerPayloadState::CompatibilityFallback` is not decorative. Treating it as
equivalent to `Converged` creates a silent downgrade path. (Protocol Evidence V3)

### 5. Completeness must be enforced, not inferred

Missing required fields must trigger fail-closed or explicit fallback — never
silent path degradation. (Protocol Evidence V4)

## Recommendations

### Preserve as invariants

- Content-addressed hashing on replay baselines and state snapshots
- Field provenance validation in context contract
- Trust-domain verification for adapter identity
- `consumer_read_path` as mandatory consumer contract
- `required_consumer_fields` completeness enforcement
- Deny convergence tier-1 precedence (`decision_outcome_kind` wins)

### Consumer/SDK guidance

- Fail-closed on missing required fields
- Follow canonical 4-tier deny precedence, not legacy flags
- Preserve all required fields in forwarding chains or explicitly signal loss
- Treat `CompatibilityFallback` as distinct from `Converged`

### What not to do

- Do not add new consumer fields without updating `required_consumer_fields`
- Do not treat provenance/compat signals as optional metadata
- Do not bypass trust-domain checks for convenience

## Practical Implications

For each layer in the trust chain:
- **Producers** must preserve field provenance — when a field was produced matters as much as its value
- **Adapters** must preserve trust-domain identity — source URN and metadata alone are not sufficient
- **Consumers** must fail-closed on missing required fields and follow canonical tier precedence

## Scope Boundary

This synthesis is bounded to deterministic structural attacks on trust interpretation.
It does not claim coverage over semantic persuasion, multi-agent planning, or
control-plane compromise. All attacks are schema-valid, protocol-plausible, and
tested without LLM involvement.

## Next Frontier

The next open question is not whether the trust chain works in isolation, but whether
external integrations and SDK consumers preserve these invariants end-to-end.

## Experiment Infrastructure (snapshot as of 2026-03-15)

| Metric | Value |
|--------|-------|
| Total attack vectors | 12 (4 per experiment) |
| Total benign controls | 9 (3 per experiment) |
| Total conditions tested | 9 (3 per experiment) |
| Total test runs | 87 (21 per matrix + overhead) |
| Total tests | 46 (unit + integration across all three) |
| False positives | 0 across all experiments and conditions (within bounded benign controls) |
| LLM calls | 0 (all deterministic structural testing) |
| Runtime pipeline changes | 0 |

## References

- [Memory Poisoning Plan](PLAN-EXPERIMENT-MEMORY-POISON-DELAYED-TRIGGER-2026q2.md)
- [Memory Poisoning Results](RESULTS-EXPERIMENT-MEMORY-POISON-2026q2.md)
- [Delegation Spoofing Plan](PLAN-EXPERIMENT-DELEGATION-SPOOFING-PROVENANCE-2026q2.md)
- [Delegation Spoofing Results](RESULTS-EXPERIMENT-DELEGATION-SPOOFING-2026q2.md)
- [Protocol Evidence Plan](PLAN-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md)
- [Protocol Evidence Results](RESULTS-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md)
