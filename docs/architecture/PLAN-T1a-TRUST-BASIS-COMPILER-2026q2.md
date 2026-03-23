# PLAN — T1a Trust Basis Compiler MVP (2026q2)

> Status: Implemented on `main` (March 2026)
> Date: 2026-03-23
> Scope: first bounded implementation wave under [ADR-033](./ADR-033-OTel-Trust-Compiler-Positioning.md) and [RFC-005](./RFC-005-trust-compiler-mvp-2026q2.md)
> Constraint: claim-first, canonical evidence first, no score/badge surface, no new signals in the same wave
> Note: This plan is execution framing for `T1a` only. `T1b` Trust Card rendering, `G3` authorization signals, and `P2` protocol claim packs stay out of scope.

## 1) Goal

Deliver the first concrete trust-compiler slice:

- input: a **verified evidence bundle**
- compiler stage: derive a small, bounded claim basis from canonical evidence
- output: a deterministic, machine-readable `trust-basis.json`

`T1a` succeeds only if claim classification happens in the trust basis/compiler layer and later render surfaces can remain semantically thin.

## 2) Architecture Guardrails

Always enforce:

- Assay is not a tracing platform, observability dashboard, eval suite, or score-first trust product
- the canonical evidence layer remains the truth substrate; raw OTel never becomes the claim authority
- `trust-basis.json` is the canonical compiler output
- `trustcard.json` and `trustcard.md` are follow-on rendering artifacts and must not appear in `T1a`
- no claim may be classified directly from raw OTel or raw upstream protocol material; classification must operate on canonical evidence present in a verified bundle
- no new signals, packs, or engine semantics are introduced in the same wave
- no aggregate trust score, safe/unsafe badge, or maturity badge appears anywhere in the output contract

## 3) Current Repo Seams T1a Must Build On

The plan should stay anchored to shipped surfaces already on `main`:

- bundle verification and reading in:
  - [crates/assay-evidence/src/bundle/reader.rs](../../crates/assay-evidence/src/bundle/reader.rs)
  - [crates/assay-evidence/src/bundle/writer_next/verify.rs](../../crates/assay-evidence/src/bundle/writer_next/verify.rs)
- canonical event contract in:
  - [crates/assay-evidence/src/types.rs](../../crates/assay-evidence/src/types.rs)
- lint and pack execution metadata in:
  - [crates/assay-evidence/src/lint/engine.rs](../../crates/assay-evidence/src/lint/engine.rs)
  - [crates/assay-evidence/src/lint/packs/executor.rs](../../crates/assay-evidence/src/lint/packs/executor.rs)
  - [crates/assay-evidence/src/lint/sarif.rs](../../crates/assay-evidence/src/lint/sarif.rs)
- shipped trust-relevant signals already available on `main`:
  - `delegated_from` on decision evidence from `G2`
  - `assay.sandbox.degraded` from `G1`
- current evidence pipeline posture documented in:
  - [docs/architecture/data-flow.md](./data-flow.md)
  - [docs/ROADMAP.md](../ROADMAP.md)

`T1a` must compose these seams; it must not reopen them.

## 4) T1a Output Contract (Frozen)

### 4.1 Canonical Artifact

`T1a` produces:

- `trust-basis.json`

`trust-basis.json` is:

- the canonical v1 compiler output
- derived from a verified bundle
- machine-readable and diff-friendly
- deterministic across regeneration from the same verified bundle
- intended to evolve additively/versionedly rather than as a throwaway MVP artifact

### 4.2 Minimal Shape

The planning shape is:

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

Per claim in `T1a`:

- `id`
- `level`
- `source`
- `boundary`
- `note`

### 4.3 Source Vocabulary (Frozen v1)

`source` must use a small fixed vocabulary in `T1a`:

- `bundle_verification`
- `bundle_proof_surface`
- `canonical_decision_evidence`
- `canonical_event_presence`
- `pack_execution_results`

No free-form `source` strings in `T1a`.

### 4.4 Boundary Vocabulary (Frozen v1)

`boundary` must use a small fixed vocabulary in `T1a`:

- `bundle-wide`
- `supported-delegated-flows-only`
- `supported-containment-fallback-paths-only`
- `proof-surfaces-only`
- `pack-execution-only`

No free-form `boundary` strings in `T1a`.

### 4.5 Determinism Rules

The canonical artifact must:

- use stable claim ordering
- regenerate deterministically from the same verified bundle
- avoid wall-clock timestamps in canonical output
- avoid host-specific or environment-specific volatile fields
- remain diff-friendly by default
- treat canonical serialization format itself as part of the contract

## 5) Claim Key Freeze (T1a v1)

`T1a` starts with a small fixed claim set:

- `bundle_verified`
- `signing_evidence_present`
- `provenance_backed_claims_present`
- `delegation_context_visible`
- `containment_degradation_observed`
- `applied_pack_findings_present`

Rules for this freeze:

- no additional claim keys in `T1a`
- no renames in the same wave
- no grouped theme-first JSON structure ahead of claim-first output
- all frozen claim keys must always be present in `trust-basis.json`, even when their `level` is `absent`

## 6) Claim Classification Rules (Initial)

Evidence levels stay aligned with ADR-033:

- `verified`
- `self_reported`
- `inferred`
- `absent`

Initial claim rules:

### `bundle_verified`
- `verified` only when bundle verification succeeds
- boundary: `bundle-wide`
- source: bundle verification result

### `signing_evidence_present`
- only classify above `absent` when direct signing/proof evidence already exists in canonical bundle/proof surfaces
- minimum positive evidence in v1: a verified bundle contains explicit signing/proof material in existing bundle proof surfaces; naming, origin text, or unsigned metadata never count
- do not infer from naming, origin text, or unrelated metadata

### `provenance_backed_claims_present`
- only classify above `absent` when existing canonical/proof surfaces directly support provenance-backed interpretation
- minimum positive evidence in v1: existing bundle proof surfaces explicitly back provenance-oriented interpretation for the relevant subject or claim set; descriptive text alone never counts
- do not imply SLSA-style provenance hardness for runtime behavior

### `delegation_context_visible`
- `verified` only when supported decision evidence contains explicit `delegated_from`
- boundary: supported delegated flows only
- source: canonical `assay.tool.decision` evidence
- no false positive from loose notes or unstructured hints

### `containment_degradation_observed`
- `verified` only when canonical evidence contains `assay.sandbox.degraded`
- boundary: supported weaker-than-requested containment fallback paths only
- source: canonical evidence event presence
- no correctness implication about sandboxing overall

### `applied_pack_findings_present`
- classify only from explicit lint/pack execution results
- semantic freeze in v1: explicit lint execution recorded at least one finding from applied packs
- do not reconstruct from SARIF prose or markdown summaries

## 7) CLI And Product Surface Posture

`T1a` is compiler-first.

Recommended posture:

- `trust-basis.json` should be exposable as a low-level CLI artifact
- preferred shape for the first explicit interface is a low-level command such as `assay trust-basis generate <bundle>`
- but `T1a` does not need to optimize for polished end-user presentation yet
- `T1b` is the first wave that should expose the iconic user-facing Trust Card command
- Trust Card rendering remains one-way derivation from `trust-basis.json`

In other words:

- `T1a` optimizes for canonicality and CI usefulness
- `T1b` optimizes for artifact legibility
- `T1a` owns claim classification; later Trust Card surfaces must not invent new claim semantics

## 8) Implementation Boundaries

Expected write scope for `T1a`:

- new trust-basis schema / model in the evidence/compiler surface
- generator path from verified bundle -> trust basis
- deterministic JSON serialization tests
- focused fixtures covering current shipped signals
- docs and reviewer gate

Expected no-touch zones:

- signal emitters
- policy engine semantics
- pack definitions
- Trust Card rendering
- dashboard/UX work
- raw OTel semantic conventions as output schema

## 9) Tests

### Contract Tests

- golden fixture proves byte-stable regeneration of `trust-basis.json`
- verified bundle produces deterministic `trust-basis.json`
- same bundle regenerates byte-stable canonical JSON
- claim ordering is stable
- all frozen claims are present even when classified as `absent`
- no wall-clock fields appear in canonical artifact
- `source` and `boundary` stay inside their frozen vocabularies

### Semantics Tests

- `delegation_context_visible` becomes `verified` only with supported `delegated_from`
- `containment_degradation_observed` becomes `verified` only with `assay.sandbox.degraded`
- self-reported-only metadata does not upgrade to `verified`
- absent evidence remains `absent` rather than guessed upward

### Boundary Tests

- no raw OTel-only fixture can produce claim classification without canonical evidence mapping
- no trust score or badge appears anywhere in compiler output
- no claim key outside the frozen set appears in v1
- Trust Card rendering fixtures must not be required for `T1a` correctness

## 10) Reviewer Gate

`T1a` should hard-fail review if the slice:

- introduces `trustcard.json` or `trustcard.md`
- introduces a trust score, safe/unsafe badge, or maturity badge
- classifies claims directly from raw OTel spans instead of canonical evidence
- adds new runtime/security signals in the same PR
- broadens pack semantics
- changes engine/policy semantics unrelated to trust basis generation
- emits non-deterministic canonical output
- grows the claim key set beyond the frozen v1 list

## 11) Acceptance

`T1a` is done when:

1. a verified bundle can produce `trust-basis.json`,
2. `trust-basis.json` is the canonical compiler output,
3. claim classification happens in the trust basis/compiler stage,
4. output is deterministic, ordered, and diff-friendly,
5. claim keys remain within the frozen v1 set,
6. no score/badge or Trust Card surface has been introduced.

## 12) Follow-On Handoff

If `T1a` lands cleanly, the next wave may be:

- `T1b` — Trust Card MVP

But only on top of the following invariant:

- `trustcard.json` and `trustcard.md` derive from `trust-basis.json`
- rendering introduces no new claim semantics
- the claim basis remains canonical
