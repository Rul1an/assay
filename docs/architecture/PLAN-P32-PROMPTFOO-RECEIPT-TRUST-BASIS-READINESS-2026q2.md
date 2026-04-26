# PLAN — P32 Promptfoo Receipt Trust Basis Readiness (Q2 2026)

- **Date:** 2026-04-26
- **Owner:** Evidence / Trust Compiler
- **Status:** Execution slice
- **Scope:** Prove that P31 Promptfoo receipt bundles are consumable by the current Trust Basis compiler without introducing a Promptfoo-specific claim row, Trust Card schema bump, or Harness regression semantics.

## 1. Why this exists

P31 made the first Promptfoo compiler path real:

- Promptfoo CLI JSONL component result in
- one Assay `EvidenceEvent` receipt per component result
- direct `BundleWriter` output
- `assay evidence verify` compatibility

The next step should not jump straight to a public thesis or a Trust Card v3
surface. The smallest useful follow-up is to lock the existing compiler path:

```text
promptfoo results.jsonl
  -> assay evidence import promptfoo-jsonl
  -> assay evidence verify
  -> assay trust-basis generate
```

That proves the imported receipts are not a dead-end sample file.

## 2. What P32 is

P32 is a readiness and regression slice.

It should prove that a Promptfoo receipt bundle can pass through the existing
Trust Basis compiler and produce deterministic `trust-basis.json` output.

The current Trust Basis claim set remains unchanged in this slice.

## 3. What P32 is not

P32 does not add:

- a Promptfoo-specific Trust Basis claim
- a Trust Card schema version bump
- new Trust Card Markdown rendering
- Assay Harness baseline/candidate comparison
- Promptfoo run pass/fail semantics
- model-output correctness semantics
- expected-value truth semantics
- Promptfoo config, prompt, output, vars, provider, token, cost, or stats truth

This matters because Trust Basis and Trust Card are compatibility surfaces.
Adding a claim row is a real schema decision, not a cleanup detail.

## 4. Current claim posture

For P32, Trust Basis may say:

- the bundle verified
- the receipt events were accepted as bundle events
- the current compiler can read the bundle and emit canonical Trust Basis JSON

For P32, Trust Basis must not yet say:

- an external evaluation receipt boundary is verified
- Promptfoo component-result provenance is verified as a named claim
- raw Promptfoo payload exclusion is a named Trust Basis claim
- any Promptfoo assertion passed at a run or system level

Those are good future candidates, but they require a deliberate claim addition.

## 5. Why not add the claim now

The tempting next claim would be something like:

```text
external_eval_receipt_boundary_visible
```

That claim could eventually summarize:

- supported external receipt event is present
- source surface is visible
- source artifact digest is present
- reducer version is visible
- raw payload fields are excluded

But adding it changes the Trust Basis claim set. Because Trust Card is derived
from Trust Basis, it also changes the Trust Card visible claim table and likely
requires a Trust Card schema bump.

P32 keeps that compatibility decision separate and explicit.

## 6. Acceptance criteria

P32 is complete when:

- a regression test imports Promptfoo CLI JSONL into a bundle
- the generated bundle verifies with `assay evidence verify`
- the same bundle generates Trust Basis JSON with `assay trust-basis generate`
- the test documents that no Promptfoo-specific Trust Basis claim is emitted yet
- CLI docs show the import -> verify -> Trust Basis flow
- docs explicitly state that Promptfoo-specific Trust Basis claims and Trust Card schema changes are follow-ups

## 7. Follow-up path

The next compatibility-expanding slice can introduce a named claim only if it
also handles the downstream contract deliberately.

Recommended next slice:

```text
P33 — External Evaluation Receipt Boundary Claim
```

That slice should decide:

- exact claim id
- exact supported event types
- payload predicate for digest, reducer, source surface, and raw-field exclusion
- Trust Card schema version impact
- migration note for consumers that key by `claim.id`

Until then, P31 receipts remain portable and Trust Basis-readable without
claim-set expansion.
