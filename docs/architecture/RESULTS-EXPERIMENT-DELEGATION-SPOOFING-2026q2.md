# Experiment Results: Delegation Capability Spoofing with Provenance Ambiguity (Q2 2026)

## Status

Experiment complete. All hypotheses tested. Closure.

## Executive Summary

The full Assay trust stack (Condition C) stops 100% of tested delegation spoofing
vectors while producing zero false positives. Integrity checks alone (Condition B)
block 3 of 4 vectors. V3 (identity spoofing) is the hardest vector — only blocked
by trust-domain verification in the full stack.

## Results Matrix

### Per-Condition

| Metric | Condition A (unprotected) | Condition B (integrity) | Condition C (full stack) |
|--------|--------------------------|------------------------|-------------------------|
| Bypassed | 4/4 (100%) | 1/4 (25%) | 0/4 (0%) |
| Detected | 0/4 | 3/4 | 4/4 |
| FPBR | 0% | 0% | 0% |

### Per-Vector

| Vector | Target | Cond A | Cond B | Cond C |
|--------|--------|--------|--------|--------|
| V1 Capability Overclaim | `AdapterCapabilities` | Trust Upgrade | Detected | Detected |
| V2 Provenance Ambiguity | `LossinessReport` | Trust Upgrade | Detected | Detected |
| V3 Identity Spoofing | `AdapterDescriptor` | Trust Upgrade | **Trust Upgrade** | Detected |
| V4 Selection Manipulation | Adapter selection | Selection Manip | Detected | Detected |

### Defense Layer Contribution

| Transition | Vectors blocked | Mechanism |
|-----------|----------------|-----------|
| A → B | V1, V2, V4 | Capability claim vs observed emission, provenance tracking, lossiness propagation |
| B → C | V3 | Trust-domain verification via adapter_id pinning |

## Hypothesis Validation

| ID | Statement | Result | Evidence |
|----|-----------|--------|----------|
| H1 | COR < 10% under Condition B | **Confirmed** | V1 detected, V4 detected |
| H2 | PBR < 5% under Condition C | **Confirmed** | V2 detected under both B and C |
| H3 | FPBR < 2% | **Confirmed** | 0/9 false positives (0.0%) |
| H4 | V3 has highest ISSR under B | **Confirmed** | V3 is the only vector that bypasses Condition B |

## Key Findings

### 1. Claim-vs-emission comparison is highly effective

V1 and V4 are both blocked under Condition B because comparing self-reported
capability claims against actually observed event emission immediately exposes
overclaims. This is a simple, cheap, and effective defense.

### 2. Provenance tracking blocks ambiguity

V2 is blocked under Condition B because tracking `raw_payload_ref` presence
and `LossinessLevel` propagation lets consumers distinguish provenance-verified
events from provenance-absent events.

### 3. Identity spoofing requires trust-domain verification

V3 is the only vector that survives Condition B. Source URN and protocol metadata
alone are not sufficient — a spoofed adapter with a different `adapter_id` but the
same `source` URN passes integrity checks. Only trust-domain verification (adapter
identity pinning) in Condition C catches this.

### 4. Zero false positives across all conditions

Controls D1 (legitimate upgrade), D2 (legitimate lossy conversion), and D3
(legitimate migration) produce no false positives under any condition.

## Structural Parallel with Memory Poisoning

| Aspect | Memory Poisoning | Delegation Spoofing |
|--------|-----------------|-------------------|
| Hardest vector | V3 (context envelope) | V3 (identity spoofing) |
| Blocked by B? | V3 survives B | V3 survives B |
| Blocked by C? | Yes (field provenance) | Yes (trust-domain verification) |
| Pattern | Schema-valid content injection | Metadata-valid identity injection |
| Key defense | Provenance-aware validation | Trust-domain-aware verification |

Both experiments show the same structural pattern: integrity checks handle most
vectors, but the hardest vector in each experiment requires a layer that validates
*origin* or *provenance*, not just content.

## Design Implications

### Must preserve

- Claim-vs-emission validation for adapter capabilities
- `raw_payload_ref` and `LossinessLevel` propagation to consumers
- Trust-domain verification for adapter identity (beyond source URN)

### Hardening recommendation

- Adapter identity pinning should be a first-class invariant
- Future adapter onboarding should require explicit trust-domain registration
- Consumer paths should distinguish "metadata-matched" from "identity-verified"

## References

- Frozen contract: [PLAN-EXPERIMENT-DELEGATION-SPOOFING-PROVENANCE-2026q2.md](PLAN-EXPERIMENT-DELEGATION-SPOOFING-PROVENANCE-2026q2.md)
- PR #869 (Step 1 freeze), PR #870 (Step 2 implementation + results)
