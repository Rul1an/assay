# Evidence Receipts from Promptfoo JSONL

Use this if Promptfoo already runs in CI and you want a smaller reviewable
artifact than a full JSONL row.

Assay does not replace Promptfoo. Promptfoo runs the assertions and writes the
JSONL output. Assay reduces selected assertion component results into bounded
evidence receipts, bundles them, verifies the bundle, and lets CI gate the
Trust Basis diff above that bundle.

## Problem

A Promptfoo CI run can tell you whether an eval passed. Later review often
needs a smaller question:

```text
Which eval outcome was selected, what source artifact did it come from, and
can that boundary be reviewed without importing the full Promptfoo run?
```

That is the receipt boundary. It is useful for pull request review, incident
follow-up, and audit trails where the reviewer should not need raw prompts,
model outputs, vars, provider metadata, or a full eval dashboard.

## One Workflow

First write Promptfoo JSONL:

```bash
promptfoo eval --output results.jsonl
```

Then import the supported assertion component results into an Assay evidence
bundle:

```bash
assay evidence import promptfoo-jsonl \
  --input results.jsonl \
  --bundle-out promptfoo-evidence.tar.gz \
  --source-artifact-ref results.jsonl
```

Verify the bundle and compile the claim artifact:

```bash
assay evidence verify promptfoo-evidence.tar.gz
assay trust-basis generate promptfoo-evidence.tar.gz \
  --out promptfoo.trust-basis.json
```

Compare the candidate Trust Basis against a baseline:

```bash
assay trust-basis diff \
  baseline.trust-basis.json \
  promptfoo.trust-basis.json \
  --format json \
  --fail-on-regression
```

In CI, the baseline Trust Basis artifact usually comes from the default branch
or a previously approved run.

Harness owns orchestration, exit codes, Markdown, and JUnit projection. The
released recipe is here:

- [Promptfoo receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/PROMPTFOO_RECEIPT_PIPELINE.md)

## Canonical Artifact

The smallest source shape is one Promptfoo CLI JSONL row with
`gradingResult.componentResults[]`:

```json
{
  "gradingResult": {
    "componentResults": [
      {
        "pass": true,
        "score": 1,
        "reason": "Assertion passed",
        "assertion": {
          "type": "equals",
          "value": "expected-output-ref:checkout-greeting"
        }
      }
    ]
  }
}
```

The current receipt lane is intentionally strict. It imports selected
`equals` assertion component results with binary scores only. The receipt
keeps the bounded result and a digest of the source artifact, not the full
Promptfoo row.

Proof artifacts are checked in under the Evidence Receipts in Action assets:

| Artifact | Role |
|---|---|
| [`candidate.results.jsonl`](../assets/evidence-receipts-in-action/promptfoo/candidate.results.jsonl) | Tiny Promptfoo source artifact |
| [`evidence.tar.gz`](../assets/evidence-receipts-in-action/promptfoo/evidence.tar.gz) | Verifiable Assay receipt bundle |
| [`trust-basis.json`](../assets/evidence-receipts-in-action/promptfoo/trust-basis.json) | Canonical claim artifact |
| [`trust-basis.diff.json`](../assets/evidence-receipts-in-action/promptfoo/trust-basis.diff.json) | Canonical CI diff artifact |
| [`trust-basis-summary.md`](../assets/evidence-receipts-in-action/promptfoo/trust-basis-summary.md) | Markdown reviewer projection |
| [`junit-trust-basis.xml`](../assets/evidence-receipts-in-action/promptfoo/junit-trust-basis.xml) | JUnit CI projection |

## Boundary

Assay may claim that a supported external eval receipt boundary is visible:

```json
{
  "id": "external_eval_receipt_boundary_visible",
  "level": "verified",
  "source": "external_evidence_receipt",
  "boundary": "supported-external-eval-receipt-events-only"
}
```

That claim means the selected Promptfoo outcome was reduced into a supported
receipt shape, carried through a verifiable bundle, and compiled into a Trust
Basis artifact.

It does not mean Assay owns Promptfoo semantics.

## Not Claimed

This path does not claim:

- the Promptfoo run passed
- the model output was correct
- the assertion was well designed
- the eval set was complete
- the application is safe
- the full Promptfoo export is Assay truth

The claim is about a reviewable evidence boundary, not eval correctness.

## Payoff Preview

The gate projection is intentionally small:

```text
Trust Basis Gate
Status: OK
Regressed claims: 0
Removed claims: 0
Unchanged claims: 10
```

The raw JSON diff remains the canonical CI artifact. Markdown and JUnit are
review projections only.

## When to Use This

Use this path when:

- Promptfoo already runs in CI
- reviewers need a portable artifact, not only a pass/fail line
- you want Trust Basis diffs and Harness gates above selected eval outcomes
- raw prompts, outputs, vars, and provider responses should stay out of the
  receipt boundary

For the longer technical explanation, see
[From Promptfoo JSONL to Evidence Receipts](../notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md).
For the three-family static proof page, see
[Evidence Receipts in Action](../notes/EVIDENCE-RECEIPTS-IN-ACTION.md).
