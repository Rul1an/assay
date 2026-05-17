# OpenFeature EvaluationDetails to CI Review Artifact

Use this if your application already emits boolean OpenFeature
`EvaluationDetails` outcomes and you want a small reviewable CI artifact above
that decision boundary.

Assay does not replace OpenFeature or inspect provider rules. OpenFeature
produces the detailed evaluation result. Assay reduces one bounded boolean
`EvaluationDetails` outcome into a runtime decision receipt, bundles it,
verifies the bundle, and lets CI gate the Trust Basis diff above that bundle.

## Problem

A runtime flag decision may be operationally important, but the full provider
context, targeting state, and rule engine internals are often too broad for a
review artifact.

The smaller review question is:

```text
Which flag decision was observed, what source artifact did it come from, and
can that decision boundary be reviewed without importing provider/runtime truth?
```

That is the receipt boundary. It is useful when reviewers need a CI artifact for
runtime decision visibility without raw evaluation context, targeting keys,
provider config, rules, metadata, or full observability traces.

## One Workflow

First write a bounded OpenFeature `EvaluationDetails` export as JSONL:

```json
{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.missing","result":{"value":false,"reason":"ERROR","error_code":"FLAG_NOT_FOUND"}}
```

Then import the supported boolean decision details into an Assay evidence
bundle:

```bash
assay evidence import openfeature-details \
  --input evaluation-details.jsonl \
  --bundle-out openfeature-evidence.tar.gz \
  --source-artifact-ref evaluation-details.jsonl
```

Verify the bundle and compile the claim artifact:

```bash
assay evidence verify openfeature-evidence.tar.gz
assay trust-basis generate openfeature-evidence.tar.gz \
  --out openfeature.trust-basis.json
```

Compare the candidate Trust Basis against a baseline:

```bash
assay trust-basis diff \
  baseline.trust-basis.json \
  openfeature.trust-basis.json \
  --format json \
  --fail-on-regression
```

In CI, the baseline Trust Basis artifact usually comes from the default branch
or a previously approved run.

Harness owns orchestration, exit codes, Markdown, and JUnit projection. The
released recipe is here:

- [OpenFeature decision receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/OPENFEATURE_DECISION_RECEIPT_PIPELINE.md)

## Artifact Chain

```text
OpenFeature EvaluationDetails JSONL
  -> assay evidence import openfeature-details
  -> evidence.tar.gz
  -> assay trust-basis generate
  -> trust-basis.json
  -> assay trust-basis diff
  -> assay.trust-basis.diff.v1
  -> assay-harness trust-basis gate/report
```

## Canonical Artifact

The upstream OpenFeature Evaluation API documents detailed evaluation as a
result structure with a flag key, value, and optional fields such as reason,
variant, flag metadata, error code, and error message. Assay consumes a
narrower exported shape for this receipt lane.

Tiny bounded source excerpt:

```json
{
  "schema": "openfeature.evaluation-details.export.v1",
  "framework": "openfeature",
  "surface": "evaluation_details",
  "target_kind": "feature_flag",
  "flag_key": "checkout.missing",
  "result": {
    "value": false,
    "reason": "ERROR",
    "error_code": "FLAG_NOT_FOUND"
  }
}
```

The current receipt lane is intentionally strict. It imports a bounded boolean
value and, when present, bounded `variant`, `reason`, and `error_code` fields.
Provider config, evaluation context, targeting keys, rules, metadata,
`error_message`, and full provider state stay outside the receipt boundary.

Proof artifacts are checked in under the Evidence Receipts in Action assets:

| Artifact | Role |
|---|---|
| [`candidate.openfeature-details.jsonl`](../assets/evidence-receipts-in-action/openfeature/candidate.openfeature-details.jsonl) | Tiny bounded OpenFeature source artifact |
| [`evidence.tar.gz`](../assets/evidence-receipts-in-action/openfeature/evidence.tar.gz) | Verifiable Assay decision receipt bundle |
| [`trust-basis.json`](../assets/evidence-receipts-in-action/openfeature/trust-basis.json) | Canonical claim artifact |
| [`trust-basis.diff.json`](../assets/evidence-receipts-in-action/openfeature/trust-basis.diff.json) | Canonical CI diff artifact |
| [`trust-basis-summary.md`](../assets/evidence-receipts-in-action/openfeature/trust-basis-summary.md) | Markdown reviewer projection |
| [`junit-trust-basis.xml`](../assets/evidence-receipts-in-action/openfeature/junit-trust-basis.xml) | JUnit CI projection |

## Decision Receipt

The reduced receipt keeps the decision boundary and source artifact digest:

```json
{
  "schema": "assay.receipt.openfeature.evaluation_details.v1",
  "source_system": "openfeature",
  "source_surface": "evaluation_details.boolean",
  "source_artifact_ref": "candidate.openfeature-details.jsonl",
  "source_artifact_digest": "sha256:56d1e1a729d93f074044069b376fc54ef4cbef16ac5b7b0576195211ffa93436",
  "reducer_version": "assay-openfeature-evaluation-details@0.1.0",
  "imported_at": "2026-04-28T09:01:00Z",
  "decision": {
    "flag_key": "checkout.missing",
    "value": false,
    "value_type": "boolean",
    "reason": "ERROR",
    "error_code": "FLAG_NOT_FOUND"
  }
}
```

## Boundary

Assay may claim that a supported external decision receipt boundary is visible:

```json
{
  "id": "external_decision_receipt_boundary_visible",
  "level": "verified",
  "source": "external_decision_receipt",
  "boundary": "supported-external-decision-receipt-events-only"
}
```

That claim means one bounded boolean OpenFeature decision outcome was reduced
into a supported receipt shape, carried through a verifiable bundle, and
compiled into a Trust Basis artifact.

It does not mean Assay owns OpenFeature semantics.

## Not Claimed

This path does not claim:

- the flag value was correct
- the targeting rules were correct
- the provider was correct or safe
- the evaluation context was imported
- OpenFeature officially supports Assay
- Assay understands full provider/runtime semantics
- application behavior is safe

The claim is about a reviewable decision boundary, not flag correctness.

## Payoff Preview

The raw diff stays the canonical CI artifact:

```json
{
  "schema": "assay.trust-basis.diff.v1",
  "summary": {
    "regressed_claims": 0,
    "removed_claims": 0,
    "unchanged_claim_count": 10,
    "has_regressions": false
  }
}
```

The Markdown projection is intentionally smaller:

```text
Trust Basis Gate
Status: OK
Regressed claims: 0
Removed claims: 0
Unchanged claims: 10
```

Markdown and JUnit are review projections only. The raw JSON diff remains the
canonical CI artifact.

## When to Use This

Use this path when:

- OpenFeature detailed evaluation output is already available or easy to export
- reviewers need a portable artifact for a runtime decision boundary
- you want Trust Basis diffs and Harness gates above selected flag decisions
- targeting context, provider state, and rule internals should stay out of the
  receipt boundary

For the upstream field reference, see the OpenFeature
[Evaluation API](https://openfeature.dev/docs/reference/concepts/evaluation-api/)
and [Flag Evaluation API specification](https://openfeature.dev/specification/sections/flag-evaluation).
For the Assay CLI import reference, see
[`assay evidence import openfeature-details`](../reference/cli/evidence.md#openfeature-details-import).
For the three-family static proof page, see
[Evidence Receipts in Action](../notes/EVIDENCE-RECEIPTS-IN-ACTION.md).
