# PLAN: Delegation Capability Spoofing with Provenance Ambiguity (Q2 2026)

- Status: Step1 freeze (docs-only)
- Date: 2026-03-15
- Owner: Security/Evidence
- Ancestor: memory-poison-delayed-trigger-2026q2
- Scope: 4 attack vectors against adapter capability claims, provenance signals, trust domain separation, and preference/selection metadata
- Constraints:
  - no new runtime engine or control plane
  - no external identity provider integration
  - no broad A2A platform work
  - deterministic structural testing only

## 1) Why this experiment

The memory poisoning experiment demonstrated that Assay's contract stack stops delayed
state attacks with DASR = 0% under Condition C. The next open frontier is not state
persistence but **inter-component trust**: can the stack maintain correct decisions when
a delegated actor or adapter misrepresents capabilities, provenance, or identity?

Recent literature positions this as a key open risk:
- NIST AI Agent Standards Initiative (Feb 2026): agent identity, authorization, interoperability
- A2ASecBench: protocol-logic and supply-chain attacks in multi-agent systems
- MCPTox/MPMA: tool and preference manipulation at the protocol level
- LDP: noisy/unconfirmed provenance can be worse than no provenance

## 2) Overarching invariant

> A delegated actor's capability claims, provenance signals, or identity metadata must
> never cause a downstream consumer to silently upgrade trust, weaken classification,
> or accept unverified claims as verified — without detectable invariant failure.

## 3) Attack surface definition

All injected payloads must be:
- **Schema-valid**: pass `ProtocolAdapter` / `AdapterBatch` / `EvidenceEvent` deserialization
- **Protocol-plausible**: could realistically be produced by a misbehaving adapter or delegate
- **Trust-affecting**: the injection shifts how downstream consumers interpret trust, capability, or provenance

## 4) Attack vectors

### Vector 1: Capability Overclaim

**Target:** `AdapterCapabilities.supported_event_types` and capability metadata in `AdapterBatch`

**Clean baseline:**
```
AdapterCapabilities {
    supported_event_types: ["tool.decision"],
    lossiness_report: { lossiness_level: Lossless, dropped_fields: [] }
}
```
Adapter claims only `tool.decision` events and lossless conversion.

**Poisoned payload:**
```
AdapterCapabilities {
    supported_event_types: ["tool.decision", "mandate.used", "approval.granted"],
    lossiness_report: { lossiness_level: Lossless, dropped_fields: [] }
}
```
Adapter claims mandate and approval capabilities it cannot actually fulfill.

**Trigger condition:** Downstream consumer trusts `mandate.used` events from this adapter
as genuine mandate evidence. Replay/diff uses the overclaimed capabilities to classify
the adapter's coverage as broader than it actually is.

**Success:** Consumer or replay diff accepts overclaimed capabilities as verified.
**Failure:** Capability claims are validated against observed event emission (claim vs production comparison); overclaim detected.
**Defended outcome:** Overclaimed types flagged or excluded from trust-sensitive paths.

"Verified capability" in this experiment means: the adapter actually emitted events of the claimed type during observed runs. Claims without matching emission are unverified.

### Vector 2: Provenance Ambiguity

**Target:** `RawPayloadRef.sha256`, `LossinessReport`, `AdapterBatch.raw_payload_ref` provenance chain

**Clean baseline:**
```
AdapterBatch {
    events: [...],
    raw_payload_ref: Some(RawPayloadRef { sha256: "abc123...", media_type: "application/json" }),
    lossiness_report: { lossiness_level: Lossless, dropped_fields: [] }
}
```
Events have verified provenance back to raw protocol payload.

**Poisoned payload:**
```
AdapterBatch {
    events: [...],
    raw_payload_ref: None,  // provenance chain broken
    lossiness_report: { lossiness_level: BestEffort, dropped_fields: ["source_id"] }
}
```
Events have no raw payload reference and acknowledge lossy conversion, but the events
themselves look schema-valid and complete.

**Trigger condition:** Replay or evidence consumer treats events without provenance
the same as events with verified provenance. A `BestEffort` lossiness level is
interpreted as equivalent to `Lossless`.

**Success:** Consumer accepts provenance-absent events with same trust as provenance-backed events.
**Failure:** Provenance absence triggers lower trust / fallback classification.
**Defended outcome:** Events without `raw_payload_ref` or with `BestEffort` lossiness
receive explicit downgraded trust markers in evidence.

### Vector 3: Delegation Identity Spoofing

**Target:** `ProtocolDescriptor.name`, `AdapterDescriptor.adapter_id`, event `source` field

**Clean baseline:**
```
Event source: "assay://adapter/acp/v1"
ProtocolDescriptor { name: "acp", version: "1.0" }
```
Events clearly attributed to a specific, trusted adapter.

**Poisoned payload:**
```
Event source: "assay://adapter/acp/v1"     // same source string
ProtocolDescriptor { name: "acp", version: "1.0" }  // same protocol
// But actually produced by a different, untrusted adapter
```
Metadata looks identical to the trusted adapter but originates from a different trust domain.

**Trigger condition:** Consumer or replay diff uses `source` and `ProtocolDescriptor`
as trust indicators without verifying the actual adapter identity chain.

**Success:** Events from the spoofed adapter are treated as equivalent to the trusted adapter.
**Failure:** Adapter identity is verified beyond metadata (e.g., via signing or pinning).
**Defended outcome:** Trust-domain mismatch detected via adapter identity verification.

Trust-domain distinction is modelled structurally/synthetically in Step 2 — the experiment
does not build real cryptographic verification but assigns trust domains as test labels
and checks whether the stack maintains separation.

### Vector 4: Preference/Selection Manipulation

**Target:** Adapter selection logic based on `AdapterCapabilities` metadata

**Clean baseline:**
Adapter A: `supported_event_types: ["tool.decision"]`, legitimate.
Adapter B: `supported_event_types: ["tool.decision", "mandate.used"]`, legitimate broader capability.

**Poisoned payload:**
Adapter C (malicious): `supported_event_types: ["tool.decision", "mandate.used", "compliance.check"]`,
claims broadest capability to be selected as preferred adapter.

**Trigger condition:** If adapter selection prefers broader capability claims,
the malicious adapter gets selected over legitimate adapters.

**Success:** Malicious adapter selected based on overclaimed capabilities.
**Failure:** Selection validated against verified capability (observed emission), not self-reported metadata.
**Defended outcome:** Selection uses verified capability evidence, not raw claims.

## 5) Benign controls

- **Control D1: Legitimate adapter upgrade** — An adapter legitimately adds new event types
  between versions. Must not be flagged as capability overclaim.
- **Control D2: Legitimate BestEffort conversion** — A protocol genuinely cannot preserve all
  fields. `BestEffort` lossiness must not be treated as provenance attack.
- **Control D3: Legitimate adapter migration** — Source string changes between adapter versions.
  Must not be flagged as identity spoofing.

## 6) Experimental conditions

### Condition A: Unprotected

- Capability claims accepted at face value
- Provenance absence not distinguished from provenance presence
- Adapter identity based on metadata only

### Condition B: Integrity-checked

- Capability claims compared against actual event production (overclaim detection)
- `raw_payload_ref` presence/absence tracked in evidence metadata
- `LossinessLevel` propagated to downstream consumers

### Condition C: Full trust stack

Condition B, plus:
- Adapter identity verification via signing or pinning (not just metadata)
- Provenance-absent events receive explicit downgraded trust markers
- Capability verification against historical adapter behavior
- Trust-domain separation enforced in evidence consumer paths

## 7) Metrics

### Primary

- **Capability Overclaim Rate (COR):** Fraction of overclaimed capabilities accepted as verified
- **Provenance Bypass Rate (PBR):** Fraction of provenance-absent events treated as provenance-backed
- **Identity Spoofing Success Rate (ISSR):** Fraction of spoofed adapter events accepted as trusted
- **Selection Manipulation Rate (SMR):** Fraction of malicious adapter selections via overclaim

### Secondary

- **FPBR (False Positive on Benign):** Fraction of controls D1/D2/D3 incorrectly flagged
- **Trust Downgrade Accuracy:** Fraction of correctly downgraded trust on ambiguous provenance

## 8) Hypotheses

- **H1:** Under Condition B, COR drops below 10% (capability overclaims caught by production comparison)
- **H2:** Under Condition C, PBR drops below 5% (provenance-absent events explicitly downgraded)
- **H3:** FPBR stays below 2% (legitimate adapter evolution not flagged)
- **H4 (falsifiable):** V3 (identity spoofing) has the highest ISSR under Condition B,
  because integrity checks verify content but not origin trust domain

## 9) Result output shape

```json
{
  "vector_id": "v1_capability_overclaim",
  "condition": "condition_c",
  "phase_a_injected": true,
  "trigger_activated": true,
  "claim_accepted": false,
  "expected_trust_level": "unverified",
  "observed_trust_level": "unverified",
  "outcome": "activation_with_correct_detection",
  "hypothesis_tags": ["H1"]
}
```

### Success taxonomy

- **no_effect:** Poison did not reach consumer/selection path
- **retained_no_activation:** Poison reached but did not shift trust/selection
- **activation_with_correct_detection:** Trust shift attempted but detected
- **activation_with_trust_upgrade:** Trust silently upgraded (invariant violation)
- **activation_with_selection_manipulation:** Malicious adapter selected (invariant violation)

## 10) Wave structure

### Step 1 (this freeze): Docs + gate only

- This plan document
- `docs/contributing/SPLIT-PLAN-experiment-delegation-spoofing.md`
- `docs/contributing/SPLIT-CHECKLIST-experiment-delegation-spoofing-step1.md`
- `scripts/ci/review-experiment-delegation-spoofing-step1.sh`

Frozen: all `crates/`, all `.github/workflows/`

### Step 2: Implementation

- `crates/assay-sim/src/attacks/delegation_spoofing.rs`
- `crates/assay-sim/tests/delegation_spoofing_invariant.rs`

Must not touch: `crates/assay-core/src/mcp/decision.rs` (no runtime decision pipeline
changes), `crates/assay-core/src/mcp/tool_call_handler/` (no enforcement changes),
`.github/workflows/`. Step 2 adds test/sim code only.

### Step 3: Closure

- Results analysis, hypothesis validation, hardening recommendations

## 11) Explicit non-goals

- No new runtime engine or control plane
- No external identity provider (Sigstore, SPIFFE/SPIRE)
- No broad A2A platform implementation
- No multi-agent orchestration runtime
- No LLM-based semantic detection
- No workflow changes
