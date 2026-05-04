# P57 Ecosystem Seeding Pack

> **Status:** paused after one Promptfoo-context follow-up; ready for broader
> controlled seeding only after a docs tag contains the primary proof page
>
> **Last updated:** 2026-05-04
> **Scope:** small repo-native sharing pack for the released evidence receipt
> surface. This is not a launch plan, campaign, partnership claim, compliance
> claim, or new product wedge.

## Core Line

Assay compiles selected external outcomes into portable evidence receipts and
bounded Trust Basis claims.

Short post line:

> We made the evidence boundary inspectable.

## Released Link Set

| Role | Link | Use When |
|---|---|---|
| Promptfoo context note | [From Promptfoo JSONL to Evidence Receipts](https://github.com/Rul1an/assay/blob/v3.9.1/docs/notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md) | Use only for an existing Promptfoo JSONL/assertion-results context. This note is present in the `v3.9.1` tag. |
| Theory | [Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory](https://github.com/Rul1an/assay/blob/v3.9.1/docs/notes/EVIDENCE-RECEIPTS-FOR-AI-OUTCOMES-RUNTIME-DECISIONS-MODEL-INVENTORY.md) | Use as the read-more link when someone asks why receipts instead of broad integrations. |
| Runnable recipe | [Promptfoo receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/PROMPTFOO_RECEIPT_PIPELINE.md) | Use when someone wants to run the smallest released recipe immediately. |

Held until a later Assay tag:

- `EVIDENCE-RECEIPTS-IN-ACTION.md` is the intended primary proof page, but it
  is not present in the public Assay `v3.9.1` tag. Do not use it in outward
  messages until a later Assay tag, expected as docs-only `v3.9.2`, contains
  the page.

Main-only second-layer note:

- [Evidence Receipt Assurance Mapping](../notes/EVIDENCE-RECEIPT-ASSURANCE-MAPPING.md)
  is useful for assurance-context replies, but it is not part of the released
  `v3.9.1` truth line yet. Do not include it in the public one-link post until
  a later Assay tag contains it.

Current allowed package:

- Promptfoo context: From Promptfoo JSONL to Evidence Receipts
- theory: Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory
- recipe: Promptfoo receipt pipeline

Post-`v3.9.2` intended package:

- proof: Evidence Receipts in Action
- theory: Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory
- recipe: Promptfoo receipt pipeline

## Repo-Native Post

Title:

```text
Evidence Receipts in Action
```

Body:

```markdown
We made the evidence boundary inspectable.

Assay now compiles selected external outcomes into portable evidence receipts,
verifiable bundles, and bounded Trust Basis claims.

The first released receipt families are intentionally small:

- Promptfoo assertion component results -> eval outcome receipts
- OpenFeature boolean EvaluationDetails -> runtime decision receipts
- CycloneDX ML-BOM machine-learning-model components -> inventory receipts

The useful part is not that Assay imports everything. It does not. The useful
part is that a small upstream outcome can become a bounded receipt, the receipt
can be bundled and verified, and the Trust Basis claim above it can be gated
without teaching Harness family-specific semantics.

Proof:
<VERSIONED_EVIDENCE_RECEIPTS_IN_ACTION_LINK>
```

Do not publish this post until `<VERSIONED_EVIDENCE_RECEIPTS_IN_ACTION_LINK>`
resolves under a public Assay tag. The expected next clean line is a small
docs-only `v3.9.2` Assay release.

Do not add:

- partnership language,
- compliance language,
- a request for general feedback,
- claims that Assay replaces Promptfoo, OpenFeature, CycloneDX, or Harness.

## Landing Spots

| Spot | Link to Send | One Sentence | Do Not Ask |
|---|---|---|---|
| Release-adjacent update | Hold until `EVIDENCE-RECEIPTS-IN-ACTION.md` is present in a public Assay tag. | We made the evidence boundary inspectable: selected Promptfoo, OpenFeature, and CycloneDX outcomes now compile into bounded receipts and Trust Basis claims. | Do not ask for generic feedback, stars, adoption, or partnership. Use a repo Discussion only if there is no suitable release/docs context. |
| Existing Promptfoo JSONL/componentResults context | [From Promptfoo JSONL to Evidence Receipts](https://github.com/Rul1an/assay/blob/v3.9.1/docs/notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md) | This keeps Promptfoo as the eval runner while showing how selected assertion component outcomes become portable evidence receipts. | Never open a fresh thread just to restate the proof. Do not imply Promptfoo endorsement, official integration, or eval correctness. Add the recipe link only if someone asks to run it. |
| Assurance / audit-context follow-up | [Evidence Receipt Assurance Mapping](../notes/EVIDENCE-RECEIPT-ASSURANCE-MAPPING.md) | This maps each released receipt family to the assurance question it can help answer and the claims it explicitly does not make. | Do not frame it as a compliance checklist or legal interpretation. Keep this internal/main-only until the mapping note is tagged. |

## Share Order

1. Keep the existing Promptfoo-context follow-up on the versioned
   `FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md` note.
2. Do not post the general repo-native note until the intended primary proof
   page is present in a public Assay tag.
3. After that tag exists, post the repo-native note first.
4. Wait at least one day.
5. Do at most one or two contextual follow-ups where there is already relevant
   conversation.
6. Prefer Promptfoo context first because the recipe is the smallest and most
   directly runnable.
7. Use OpenFeature or CycloneDX context later only when there is existing
   discussion about runtime decisions or model inventory boundaries.
8. Stop after those follow-ups unless someone asks for a concrete artifact,
   recipe, or boundary explanation.
9. If there is no reply or no natural continuation, stop. Do not open a new
   thread to restate the same proof.

## Guardrails

- No new receipt families.
- No new Harness behavior.
- No compliance claims.
- No partnership language.
- No broad campaign language.
- No "would love feedback" framing.
- No pressure to adopt.
- No outward link to `EVIDENCE-RECEIPTS-IN-ACTION.md` until it is included in
  a tagged Assay release.
- Do not promote the mapping note in any outward message until it is included
  in a tagged Assay release.

The goal is only to put the released proof, theory, and runnable recipe in
front of people who already care about evidence boundaries.
