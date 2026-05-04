# P57 Ecosystem Seeding Pack

> **Status:** ready for controlled seeding after Assay `v3.9.2`; one
> Promptfoo-context follow-up has already been placed
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
| Primary proof | [Evidence Receipts in Action](https://github.com/Rul1an/assay/blob/v3.9.2/docs/notes/EVIDENCE-RECEIPTS-IN-ACTION.md) | Use as the one-link repo-native proof entrypoint. This page is present in the `v3.9.2` tag. |
| Assurance mapping | [Evidence Receipt Assurance Mapping](https://github.com/Rul1an/assay/blob/v3.9.2/docs/notes/EVIDENCE-RECEIPT-ASSURANCE-MAPPING.md) | Use only for assurance-context replies after someone asks what review question each receipt family helps answer. |
| Promptfoo context note | [From Promptfoo JSONL to Evidence Receipts](https://github.com/Rul1an/assay/blob/v3.9.1/docs/notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md) | Use only for an existing Promptfoo JSONL/assertion-results context. This note is present in the `v3.9.1` tag. |
| Theory | [Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory](https://github.com/Rul1an/assay/blob/v3.9.1/docs/notes/EVIDENCE-RECEIPTS-FOR-AI-OUTCOMES-RUNTIME-DECISIONS-MODEL-INVENTORY.md) | Use as the read-more link when someone asks why receipts instead of broad integrations. |
| Runnable recipe | [Promptfoo receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/PROMPTFOO_RECEIPT_PIPELINE.md) | Use when someone wants to run the smallest released recipe immediately. |

Now tagged in Assay `v3.9.2`:

- `EVIDENCE-RECEIPTS-IN-ACTION.md` is the primary proof page.
- `EVIDENCE-RECEIPT-ASSURANCE-MAPPING.md` is the second-layer assurance note.

Current allowed package:

- proof: Evidence Receipts in Action
- Promptfoo context: From Promptfoo JSONL to Evidence Receipts
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
https://github.com/Rul1an/assay/blob/v3.9.2/docs/notes/EVIDENCE-RECEIPTS-IN-ACTION.md
```

The proof link resolves under the public Assay `v3.9.2` tag. The existing `v*`
release workflow builds full release artifacts, so do not describe `v3.9.2` as
docs-only.

Do not add:

- partnership language,
- compliance language,
- a request for general feedback,
- claims that Assay replaces Promptfoo, OpenFeature, CycloneDX, or Harness.

## Landing Spots

| Spot | Link to Send | One Sentence | Do Not Ask |
|---|---|---|---|
| Release-adjacent update | [Evidence Receipts in Action](https://github.com/Rul1an/assay/blob/v3.9.2/docs/notes/EVIDENCE-RECEIPTS-IN-ACTION.md) | We made the evidence boundary inspectable: selected Promptfoo, OpenFeature, and CycloneDX outcomes now compile into bounded receipts and Trust Basis claims. | Do not ask for generic feedback, stars, adoption, or partnership. Use a repo Discussion only if there is no suitable release/docs context. |
| Existing Promptfoo JSONL/componentResults context | [From Promptfoo JSONL to Evidence Receipts](https://github.com/Rul1an/assay/blob/v3.9.1/docs/notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md) | This keeps Promptfoo as the eval runner while showing how selected assertion component outcomes become portable evidence receipts. | Never open a fresh thread just to restate the proof. Do not imply Promptfoo endorsement, official integration, or eval correctness. Add the recipe link only if someone asks to run it. |
| Assurance / audit-context follow-up | [Evidence Receipt Assurance Mapping](https://github.com/Rul1an/assay/blob/v3.9.2/docs/notes/EVIDENCE-RECEIPT-ASSURANCE-MAPPING.md) | This maps each released receipt family to the assurance question it can help answer and the claims it explicitly does not make. | Do not frame it as a compliance checklist or legal interpretation. |

## Share Order

1. Keep the existing Promptfoo-context follow-up on the versioned
   `FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md` note.
2. Post the repo-native note first, using only the versioned `v3.9.2` proof
   link as the primary outward link.
3. Let the proof page act as the hub for theory, mapping, and recipes.
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
- Link outward to `EVIDENCE-RECEIPTS-IN-ACTION.md` only through the versioned
  `v3.9.2` URL or a later release tag.
- Link outward to the mapping note only through the versioned `v3.9.2` URL or a
  later release tag.

The goal is only to put the released proof, theory, and runnable recipe in
front of people who already care about evidence boundaries.
