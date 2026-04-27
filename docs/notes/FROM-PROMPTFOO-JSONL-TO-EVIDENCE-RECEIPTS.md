# From Promptfoo JSONL to Evidence Receipts

> **Status:** technical note
> **Last updated:** 2026-04-27
> **Scope:** explains the existing Promptfoo receipt pipeline; adds no new schema, claim, or Harness semantics

Promptfoo already makes AI behavior testable in CI. Assay shows how selected
assertion outcomes can become portable evidence.

This note is about evidence portability for AI eval outcomes. Promptfoo is the
first concrete wedge, not the scope of the idea.

## The Gap

AI eval systems are good at running tests and producing CI outcomes. That is
necessary, but it is not the same as preserving a small, reviewable evidence
artifact for the outcome that mattered.

For teams that need later review, incident analysis, or governance workflows,
the important question is often not "did the eval tool run?" It is:

- which outcome was selected for review,
- where did it come from,
- what was reduced out,
- which compiler version produced the artifact,
- and can another tool consume that artifact without understanding the whole
  eval runner.

That is the evidence portability gap.

The point is not to re-explain eval outputs. It is to show how a selected eval
outcome becomes a portable evidence unit.

## The Boundary

Assay does not replace Promptfoo and does not re-run Promptfoo evals.

The boundary is:

- Promptfoo runs assertions, evals, CI checks, and security workflows.
- Assay imports one supported Promptfoo assertion component result into a
  portable evidence receipt.
- Trust Basis classifies whether the external eval receipt boundary is visible.
- Assay Harness can gate and report the resulting Trust Basis diff.

This is not a Promptfoo integration claim, partnership claim, or endorsement
claim. It is a downstream recipe over public Promptfoo JSONL and assertion
surfaces.

## The Artifact Unit

The first supported unit is one Promptfoo CLI JSONL assertion component result:

```text
Promptfoo CLI JSONL row -> gradingResult.componentResults[] -> one component
```

That is intentionally smaller than:

- a full JSONL row,
- a full eval run,
- a Promptfoo JSON/YAML/XML export schema,
- a red-team report,
- a provider response,
- raw prompt, output, expected value, vars, token, cost, or stats payloads.

The first lane is deterministic-first: `equals` component results with binary
scores only. That keeps the receipt boundary away from model-graded,
LLM-as-judge, rubric, or red-team semantics until those have their own bounded
surface and failure model.

## The Compiler Path

The current path is:

```text
Promptfoo CLI JSONL
  -> assay evidence import promptfoo-jsonl
  -> Assay evidence bundle
  -> assay trust-basis generate
  -> Trust Basis JSON
  -> assay trust-basis diff
  -> assay.trust-basis.diff.v1
  -> Assay Harness gate/report
```

The receipt bundle is the portable Assay evidence artifact. The Trust Basis
artifact is the canonical claim-level compiler output. The raw
`assay.trust-basis.diff.v1` JSON is the canonical diff artifact for CI
consumers.

Markdown, JUnit, job summaries, and later review projections are derived views.
They are useful for humans and CI systems, but they are not the source of
truth.

## What Trust Basis May Say

For the current Promptfoo receipt pipeline, Trust Basis may say:

- a supported external eval receipt boundary is visible,
- the receipt was carried through a verifiable Assay bundle,
- the raw Promptfoo payload was not imported into the Trust Basis claim.

It must not say:

- the Promptfoo run passed,
- the model output was correct,
- the application is safe,
- the eval was complete,
- the provider response is trustworthy,
- or the whole Promptfoo export is Assay truth.

The claim is about evidence boundary visibility, not eval correctness.

## Why This Is Not Observability

Traces and observability events are useful for debugging. They are not
automatically bounded evidence boundaries.

The Assay receipt path deliberately reduces a selected outcome into a smaller
artifact with explicit provenance and exclusion rules. That is different from
importing a trace, platform run, or full export envelope.

OpenTelemetry GenAI conventions are useful context for the ecosystem, but the
current receipt lane does not depend on them. Assay keeps this path as a
compiler boundary over a selected result surface, not a general telemetry
mapping.

## Why This Is Not Compliance Theater

Governance and audit workflows may benefit from portable evidence, but this
note does not lead with a compliance claim.

The product claim is narrower:

```text
selected eval outcome -> portable evidence receipt -> claim-level artifact
```

That makes later review possible without turning Assay into a compliance
dashboard, a Promptfoo viewer, or an eval-result truth oracle.

## Try It

The runnable recipe lives in Assay Harness:

- [Promptfoo receipt pipeline recipe](https://github.com/Rul1an/Assay-Harness/blob/main/docs/PROMPTFOO_RECEIPT_PIPELINE.md)
- [Recipe script](https://github.com/Rul1an/Assay-Harness/blob/main/demo/run-promptfoo-receipt-pipeline.sh)

The Assay-side CLI entry points are:

- [`assay evidence import promptfoo-jsonl`](../reference/cli/evidence.md#promptfoo-jsonl-import)
- [`assay trust-basis diff`](../reference/cli/trust-basis.md#diff)

The discovery sample that froze the first component-result lane is:

- [Promptfoo Assertion GradingResult Evidence Sample](../../examples/promptfoo-assertion-grading-result-evidence/README.md)

## What P39 Adds

P39 adds no new receipt schema, no new Trust Basis claim, and no new Harness
semantics. It explains the existing pipeline after the compiler path and
recipe are already present.

## References

- [Promptfoo output formats](https://www.promptfoo.dev/docs/configuration/outputs/)
- [Promptfoo deterministic assertions](https://www.promptfoo.dev/docs/configuration/expected-outputs/deterministic/)
- [Promptfoo CI/CD integration](https://www.promptfoo.dev/docs/integrations/ci-cd/)
- [NIST AI 600-1, Generative AI Profile](https://doi.org/10.6028/NIST.AI.600-1)
- [OpenTelemetry GenAI semantic conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/)
- [GitHub SARIF support for code scanning](https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning)
