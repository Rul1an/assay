# RFC-005: Trust Compiler MVP and Trust Card (Q2 2026)

- Status: Active (`T1a`, `T1b`, `G3`, `P2a`, and `H1` are public in `v3.3.0`; `G4-A`, `P2c`, and `K1-A` Phase 1 are now public in `v3.4.0`; `K1` remains the next bounded evidence wave beyond that first public slice)
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

Current delivery status on `main`:

- `T1a` is merged on `main` as the canonical `trust-basis.json` compiler output and low-level `assay trust-basis generate` surface
- `T1b` is merged on `main` as deterministic `trustcard.json` / `trustcard.md` (`assay trustcard generate`) derived from the trust basis, without a second semantic classification layer
- `G3` is merged on `main` as bounded authorization-context fields on supported MCP `assay.tool.decision` evidence (`auth_scheme`, `auth_issuer`, `principal`) with normalization that rejects JWT/Bearer credential material; Trust Basis emits **seven** claims (adds `authorization_context_visible`), and Trust Card JSON uses **`schema_version` `2`** (see [PLAN-G3](./PLAN-G3-AUTHORIZATION-CONTEXT-EVIDENCE-2026q2.md))

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
- **the preferred order stays `T1a -> T1b -> G3 -> P2 -> K1` before any broader next-pack expansion**
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

Detailed execution framing for the first wave lives in [PLAN — T1a Trust Basis Compiler MVP](./PLAN-T1a-TRUST-BASIS-COMPILER-2026q2.md).

### 4.1 Objective

Make the compiler model explicit:

- **inputs**: OTel exports, Assay traces, protocol/runtime evidence
- **compiler stage**: canonicalization, mapping, bounded claim basis
- **outputs**: verifiable bundles plus a bounded machine-readable trust basis for higher-level artifacts

`T1a` should produce a concrete canonical compiler-output artifact derived from a verified bundle.
Working name for planning: `trust-basis.json`.

### 4.2 In Scope

- define and document the official trust-compiler input surfaces already supported on `main`
- define a bounded machine-readable claim basis contract that can be derived from a verified bundle
- wire the compiler story through existing CLI and docs, not through a dashboard
- keep the existing `ProfileCollector -> EvidenceMapper -> EvidenceEvent` architecture as the base path
- treat OTel exports as inputs that map into Assay's canonical evidence layer rather than as the final truth contract
- ensure new ingest paths are additive/translational and cannot semantically overrule claim classification outside canonical evidence
- make the trust basis the place where claim classification happens, rather than in later rendering layers
- keep `trust-basis.json` as the canonical compiler output that later Trust Card artifacts derive from

### 4.3 Out of Scope

- replacing the existing bundle flow
- emitting CloudEvents directly in the hot path
- broadening pack semantics
- introducing a collector-side processor/runtime before the claim basis contract is stable

### 4.4 Acceptance

`T1a` is successful if:

- Assay has a documented trust-compiler contract grounded in existing evidence surfaces
- a verified bundle can produce a deterministic bounded machine-readable trust basis artifact
- the trust basis uses evidence-level classifications instead of a single scalar score
- existing bundle verification and lint flows remain the source of truth for lower-level evidence
- `trust-basis.json` is treated as the canonical compiler output rather than an incidental intermediate file

### 4.5 Trust Basis Shape (MVP Planning Freeze)

`T1a` does not need a final schema in this RFC, but it should converge on a small claim-first shape such as:

```json
{
  "claims": [
    {
      "id": "bundle_verified",
      "level": "verified",
      "source": "bundle_verification",
      "boundary": "bundle-wide",
      "note": null
    }
  ]
}
```

Planning constraints for the trust basis:

- each item has a stable claim key / `id`
- claim classification uses `verified`, `self_reported`, `inferred`, or `absent`
- each claim identifies its evidence source
- each claim identifies its supported-flow or scope boundary
- free-form notes remain optional and non-authoritative
- canonical JSON output should use stable ordering and deterministic regeneration
- the canonical artifact should not include wall-clock timestamps or other host-specific volatile fields
- the canonical artifact should remain diff-friendly by default

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

`trustcard.json` is the canonical Trust Card artifact derived from `trust-basis.json`. `trustcard.md` is a deterministic human-readable rendering of the same claim set.

### 5.2 Trust Card Claim Set

Trust Card rendering must not invent new claim semantics. Claim classification happens in the trust basis / compiler stage, not in Markdown rendering.

The first Trust Card should stay small and explicitly bounded. The shipped claim set is **seven** rows (fixed order in the trust basis; consumers must key by `id`, not positional length):

- `bundle_verified`
- `signing_evidence_present`
- `provenance_backed_claims_present`
- `delegation_context_visible`
- `authorization_context_visible` (`G3`; Trust Card JSON `schema_version` **2**)
- `containment_degradation_observed`
- `applied_pack_findings_present`

These claims may then be rendered into grouped sections for human readability, but the claim set should come first.

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
- `trustcard.json` remains the canonical artifact and `trustcard.md` remains a deterministic rendering of the same claim set

## 6. Follow-On Sequencing

After `T1a`, `T1b`, and `G3` on `main`, the preferred sequence is:

1. `P2` — Protocol Claim Packs — **first slice `P2a`**: built-in companion pack `mcp-signal-followup` (MCP-001..003; MCP-001 shares G3 semantics with Trust Basis via `g3_authorization_context_present` in pack engine v1.2; see [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md))
2. **`H1` — Trust kernel alignment & release hardening** (before broadening protocol packs): shared predicate / classifier / Trust Card / pack / CLI alignment tests; single migration SSOT — see [PLAN-H1](PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md), [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md)
3. further `P2` slices — **`P2b`**: built-in `a2a-signal-followup` (A2A-001..003; presence-only on `assay.adapter.a2a.*` per shipped adapter; [PLAN-P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)); see also [ROADMAP](../ROADMAP.md)

*Steps 4–6 continue the sequence **after** `P2b` is on `main`. Step 7 remains explicitly deferred.*

4. **`G4` — A2A discovery / card evidence signal** (evidence-wave before the next A2A pack slice; adapter-first; not a companion pack): see [PLAN-G4](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md)
5. **`P2c` — A2A discovery/card follow-up pack** (productization after G4; companion pack rules aligned to G4 evidence — built-in `a2a-discovery-card-followup`, now public in **`v3.4.0`**; [PLAN-P2c](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md))
6. **`K1` — A2A handoff / delegation-route visibility evidence** (next formal wave after `P2c`; adapter-first; no pack in the same slice). `K1-A` Phase 1 is now the first public bounded A2A adapter seam in **`v3.4.0`**; see [PLAN-K1](PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md) and [K1-A freeze](K1-A-PHASE1-FREEZE.md)
7. only later: further pack slices if `K1` or another evidence wave makes them honest; reference existence, temporal validity, capability attestation, richer compliance packs

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
- What is the minimal stable claim key set for Trust Card v1?
