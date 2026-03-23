# RFC-005: Trust Compiler MVP and Trust Card (Q2 2026)

- Status: Proposed
- Date: 2026-03-23
- Owner: Evidence / Product
- Scope: bounded execution framing for `T1a` and `T1b`
- Inputs:
  - [ADR-033: OTel-Native Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
  - [ADR-006: Evidence Contract](./ADR-006-Evidence-Contract.md)
  - [ADR-008: Evidence Streaming](./ADR-008-Evidence-Streaming.md)
  - [ADR-025: Evidence-as-a-Product](./ADR-025-Evidence-as-a-Product.md)
  - [ADR-026: Protocol Adapters](./ADR-026-Protocol-Adapters.md)
  - [OWASP Agentic A1/A3/A5 C1 Mapping](../security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md)
  - [SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2](./SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md)

## 1. Context

Assay now has enough substrate on `main` to turn a documentation thesis into a bounded product line:

- OTel-style ingest and transformation already exist
- evidence bundles and offline verification are shipped
- signal-aware pack surfaces now exist for supported delegation context and supported containment degradation
- the last release-line truth issue left after `P1` is closed on `main` via `3.2.3`

What is missing is a productized bridge from "we collect evidence" to "we emit reviewable trust claims."

This RFC defines that bridge in two steps:

1. `T1a` — make the evidence compiler line explicit and bounded
2. `T1b` — generate a Trust Card as the first iconic output artifact

## 1.5 Why This Wedge And Not The Alternatives

This RFC assumes the following strategic comparison is already resolved:

- **not another pack-first move**: packs matter, but they are downstream of claim and signal reality
- **not another engine-first move**: heavier semantics are valuable later, but they are not the sharpest next wedge
- **not dashboard-first**: dashboards help explain results, but they do not create the distinctive product category

The Trust Compiler + Trust Card path is chosen because it best converts already-shipped evidence capabilities into a tangible product surface.

## 2. Goals

### Goal A — Productize Assay as a compiler, not a dashboard

The MVP should clarify that Assay turns runtime truth into:

- canonical evidence
- bounded claim classification
- portable proof-bearing outputs

### Goal B — Make claim provenance legible to humans and machines

The first public-facing artifact should tell reviewers what is:

- `verified`
- `self_reported`
- `inferred`
- `absent`

without requiring them to reverse-engineer bundle details by hand.

### Goal C — Preserve the bounded-claims discipline from `C2`, `E1`, `G1`, `G2`, and `P1`

The MVP must not overclaim delegation validation, sandbox correctness, temporal correctness, or broad protocol security that the evidence cannot support.

## 2.5 North Star Constraints

This RFC inherits the following hard constraints from ADR-033:

- **claim-first, not dashboard-first**
- **Assay canonical evidence is the truth layer**
- **OTel is a first-class ingest path, not the sole semantic authority**
- **Trust Card is evidence-classified, not score-first**
- **the preferred order stays `T1a -> T1b -> G3 -> P2`**
- **Assay is not a tracing platform, eval platform, or observability dashboard**

## 3. Non-Goals

This RFC does not include:

- a tracing platform
- a general observability dashboard
- eval-as-a-service
- a generic risk score as the primary output
- new pack semantics in the same slice
- new signal emitters in the same slice
- delegation-chain validation, cryptographic chain verification, or temporal/reference semantics
- a fully general OTel Collector processor before the basic compiler and Trust Card contracts are stable
- any primary `trusted/untrusted` or scalar trust-score output
- any primary aggregate maturity badge or badge-first UX

## 3.5 Primary Risks

- **Abstractness**: without a concrete artifact, `trust compiler` is too architectural. `T1b` exists to solve this.
- **Category drift**: it will be tempting to add dashboard-first or score-first surfaces because they are easier to market. This RFC explicitly rejects that as the main product lane.
- **Standards volatility**: OTel/agent semconv will keep moving. Compiler inputs may evolve, but Assay's canonical evidence layer must remain the stable claim substrate.

## 4. T1a — OTel-Native Trust Compiler MVP

### 4.1 Objective

Make the compiler model explicit:

- **inputs**: OTel exports, Assay traces, protocol/runtime evidence
- **compiler stage**: canonicalization, mapping, bounded claim basis
- **outputs**: verifiable bundles plus machine-readable trust-basis data for higher-level artifacts

### 4.2 In Scope

- define and document the official trust-compiler input surfaces already supported on `main`
- define a bounded machine-readable claim basis contract that can be derived from a verified bundle
- wire the compiler story through existing CLI and docs, not through a dashboard
- keep the existing `ProfileCollector -> EvidenceMapper -> EvidenceEvent` architecture as the base path
- treat OTel exports as inputs that map into Assay's canonical evidence layer rather than as the final truth contract
- ensure new ingest paths are additive/translational and cannot semantically overrule claim classification outside canonical evidence

### 4.3 Out of Scope

- replacing the existing bundle flow
- emitting CloudEvents directly in the hot path
- broadening pack semantics
- introducing a collector-side processor/runtime before the claim basis contract is stable

### 4.4 Acceptance

`T1a` is successful if:

- Assay has a documented trust-compiler contract grounded in existing evidence surfaces
- a verified bundle can produce a bounded machine-readable trust basis
- the trust basis uses evidence-level classifications instead of a single scalar score
- existing bundle verification and lint flows remain the source of truth for lower-level evidence

## 5. T1b — Trust Card MVP

### 5.1 Objective

Create the first iconic output artifact of the trust-compiler line.

Suggested CLI:

```bash
assay trustcard generate bundle.tar.gz
```

Suggested outputs:

- `trustcard.json`
- `trustcard.md`

### 5.2 Trust Card Sections

The first Trust Card should stay small and explicitly bounded. It should summarize whether the bundle:

- verifies offline
- carries signing / trust-domain evidence
- distinguishes provenance-backed vs provenance-absent claims
- surfaces delegation context for supported flows
- surfaces containment degradation for supported fallback paths
- records applied packs and their bounded findings
- declares supported-flow boundaries and non-goals

### 5.3 Trust Card Evidence Levels

Each claim section should classify its status using the evidence levels from ADR-033:

- `verified`
- `self_reported`
- `inferred`
- `absent`

Optional future summaries may exist, but the primary Trust Card artifact must remain evidence-classified first.

### 5.4 Non-Goals

The Trust Card must not:

- present itself as a universal security rating
- imply chain integrity or chain completeness when only delegation context is visible
- imply sandbox correctness when only degradation evidence is present
- imply temporal/reference correctness that the engine does not support

### 5.5 Acceptance

`T1b` is successful if:

- a verified bundle can produce `trustcard.json` and `trustcard.md`
- the Trust Card contains explicit evidence-level classifications
- the language remains bounded and non-goal-aware
- the Trust Card can be regenerated deterministically from the same verified bundle
- the Trust Card does not reduce the primary result to a scalar score or binary `trusted/untrusted` label

## 6. Follow-On Sequencing

After `T1a` and `T1b`, the preferred sequence is:

1. `G3` — Authorization Evidence Signal
2. `P2` — Protocol Claim Packs
3. only later: reference existence, temporal validity, capability attestation, richer compliance packs

## 7. Review Gates For Future Execution

Future implementation slices under this RFC should hard-fail review if they:

- turn the MVP into a tracing platform
- turn the MVP into a dashboard project
- introduce a magic trust score as the primary product surface
- treat raw OTel semconv as the final trust schema instead of mapping through canonical evidence
- add new protocol/security claims without backing signals
- conflate delegation visibility with delegation validation
- conflate degradation visibility with containment correctness
- claim the compiler is a general observability replacement or generic eval suite

## 8. Open Questions

- What is the smallest stable machine-readable "trust basis" schema that can sit between bundle verification and Trust Card generation?
- Should the first collector-native deployment form factor be a CLI compiler only, or a sidecar/processor once the trust basis contract is stable?
- Which minimal claim sections should be mandatory for `trustcard.json` v1?
