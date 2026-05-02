# Evidence Receipts in Action

> **Status:** static proof page
> **Last updated:** 2026-04-30
> **Scope:** demonstrates the released Assay `v3.9.1` and Assay Harness
> `v0.3.2` compiler path over the three released receipt families. This page
> adds no new receipt schema, Trust Basis claim, Harness behavior, hosted demo,
> or upstream integration claim.

Selected external outcomes can become portable evidence receipts.

Promptfoo, OpenFeature, and CycloneDX are the first three released receipt
families. This page shows the small proof path: selected upstream output,
bounded Assay receipt, verified bundle, Trust Basis claim, and Harness
gate/report artifacts.

The useful thing is not that Assay imports everything. It does not. The useful
thing is that Assay keeps the evidence unit small enough to review. Small
receipts, real artifacts, no extra theater.

## What Was Tested. What Was Decided. What the System Was Built With.

| Question | Source surface | Receipt family | Trust Basis claim |
|---|---|---|---|
| What was tested? | Selected Promptfoo assertion component result | eval outcome receipt | `external_eval_receipt_boundary_visible` |
| What was decided? | Boolean OpenFeature `EvaluationDetails` outcome | runtime decision receipt | `external_decision_receipt_boundary_visible` |
| What was the system built with? | Selected CycloneDX `machine-learning-model` component | inventory / provenance receipt | `external_inventory_receipt_boundary_visible` |

This is not a claim that Assay owns the truth of Promptfoo, OpenFeature, or
CycloneDX. It claims only that selected bounded outcomes can be reduced into
portable receipts and then compiled into Trust Basis artifacts.

The checked-in proof artifacts for this page live under
[`docs/assets/evidence-receipts-in-action/`](../assets/evidence-receipts-in-action/manifest.json).
They were generated from the Assay `v3.9.1` release binary and the Assay
Harness `v0.3.2` gate/report surface.

These checked-in artifacts are the primary proof source for this page.
Workflow runs are useful secondary proof, but they are run-scoped and
retention-bound.

## Three Receipt Families

These are the currently released receipt families only, not a promise that
every external surface will be modeled this way.

### Promptfoo: selected assertion component result

Tiny source excerpt:

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

Reduced Assay receipt payload:

```json
{
  "schema": "assay.receipt.promptfoo.assertion-component.v1",
  "source_system": "promptfoo",
  "source_surface": "cli-jsonl.gradingResult.componentResults",
  "source_artifact_ref": "candidate.results.jsonl",
  "source_artifact_digest": "sha256:299964351a22c559b83c93259da7c710d76bd50cfaf985aa039ff54558f1b68b",
  "reducer_version": "assay-promptfoo-jsonl-component-result@0.1.0",
  "imported_at": "2026-04-30T09:01:00Z",
  "assertion_type": "equals",
  "result": {
    "pass": true,
    "reason": "Assertion passed",
    "score": 1
  }
}
```

Trust Basis claim:

```json
{
  "id": "external_eval_receipt_boundary_visible",
  "level": "verified",
  "source": "external_evidence_receipt",
  "boundary": "supported-external-eval-receipt-events-only"
}
```

Artifact links:
[`candidate.results.jsonl`](../assets/evidence-receipts-in-action/promptfoo/candidate.results.jsonl),
[`evidence.tar.gz`](../assets/evidence-receipts-in-action/promptfoo/evidence.tar.gz),
[`evidence-show.json`](../assets/evidence-receipts-in-action/promptfoo/evidence-show.json),
[`trust-basis.json`](../assets/evidence-receipts-in-action/promptfoo/trust-basis.json).

### OpenFeature: boolean EvaluationDetails

Tiny source excerpt:

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

Reduced Assay receipt payload:

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

Trust Basis claim:

```json
{
  "id": "external_decision_receipt_boundary_visible",
  "level": "verified",
  "source": "external_decision_receipt",
  "boundary": "supported-external-decision-receipt-events-only"
}
```

Artifact links:
[`candidate.openfeature-details.jsonl`](../assets/evidence-receipts-in-action/openfeature/candidate.openfeature-details.jsonl),
[`evidence.tar.gz`](../assets/evidence-receipts-in-action/openfeature/evidence.tar.gz),
[`evidence-show.json`](../assets/evidence-receipts-in-action/openfeature/evidence-show.json),
[`trust-basis.json`](../assets/evidence-receipts-in-action/openfeature/trust-basis.json).

### CycloneDX ML-BOM: selected machine-learning-model component

Tiny source excerpt:

```json
{
  "bom-ref": "pkg:huggingface/example/checkout-risk-model@def456",
  "type": "machine-learning-model",
  "name": "checkout-risk-model",
  "version": "1.1.0",
  "modelCard": {
    "bom-ref": "model-card-checkout-risk-model-v1-1",
    "modelParameters": {
      "datasets": [{ "ref": "component-checkout-risk-training-data-v2" }]
    }
  }
}
```

Reduced Assay receipt payload:

```json
{
  "schema": "assay.receipt.cyclonedx.mlbom-model-component.v1",
  "source_system": "cyclonedx",
  "source_surface": "bom.components[type=machine-learning-model]",
  "source_artifact_ref": "candidate.cdx.json",
  "source_artifact_digest": "sha256:6b0618708f49e3da21bda99a5dc82ce5409cbaa2e39d152b42fc90bc70f694ac",
  "reducer_version": "assay-cyclonedx-mlbom-model-component@0.1.0",
  "imported_at": "2026-04-28T10:01:00Z",
  "model_component": {
    "bom_ref": "pkg:huggingface/example/checkout-risk-model@def456",
    "name": "checkout-risk-model",
    "version": "1.1.0",
    "dataset_refs": ["component-checkout-risk-training-data-v2"]
  }
}
```

Trust Basis claim:

```json
{
  "id": "external_inventory_receipt_boundary_visible",
  "level": "verified",
  "source": "external_inventory_receipt",
  "boundary": "supported-external-inventory-receipt-events-only"
}
```

Artifact links:
[`candidate.cdx.json`](../assets/evidence-receipts-in-action/cyclonedx/candidate.cdx.json),
[`evidence.tar.gz`](../assets/evidence-receipts-in-action/cyclonedx/evidence.tar.gz),
[`evidence-show.json`](../assets/evidence-receipts-in-action/cyclonedx/evidence-show.json),
[`trust-basis.json`](../assets/evidence-receipts-in-action/cyclonedx/trust-basis.json).

## Why the Artifact Unit Is Small

Receipts are useful here because they exclude more than they include.

| Family | Included in v1 | Excluded in v1 |
|---|---|---|
| Promptfoo | assertion type, binary pass/score, bounded reason when safe, source artifact digest | raw prompt, raw output, expected value expansion, vars, full JSONL row |
| OpenFeature | boolean value, flag key, variant/reason/error code when present, source artifact digest | targeting context, provider internals, provider config, full rule state |
| CycloneDX ML-BOM | selected model component refs, bounded model/dataset/card refs, source artifact digest | full BOM graph, full model-card body, dataset bodies, vulnerabilities, licenses, fairness/ethics sections |

That is the whole point. Assay does not need to become the upstream runner,
feature-flag system, or BOM platform. It only needs to preserve a small,
reviewable boundary and compile the claim above it.

## Canonical Artifacts Vs Projections

```text
external input
  -> assay evidence import ...
  -> evidence bundle
  -> assay trust-basis generate
  -> trust-basis.json
  -> assay trust-basis diff
  -> assay.trust-basis.diff.v1
  -> assay-harness trust-basis gate/report
```

Canonical artifacts are the bundle, Trust Basis JSON, and raw diff JSON.
Markdown and JUnit are projections only. They are useful in CI, but they do not
become the source of truth. Keep it simple: one machine artifact, thin views
above it.

The Promptfoo proof artifacts show that split:

| Artifact | Role |
|---|---|
| [`evidence.tar.gz`](../assets/evidence-receipts-in-action/promptfoo/evidence.tar.gz) | Verifiable receipt bundle |
| [`trust-basis.json`](../assets/evidence-receipts-in-action/promptfoo/trust-basis.json) | Canonical claim artifact |
| [`trust-basis.diff.json`](../assets/evidence-receipts-in-action/promptfoo/trust-basis.diff.json) | Canonical diff contract, `assay.trust-basis.diff.v1` |
| [`trust-basis-summary.md`](../assets/evidence-receipts-in-action/promptfoo/trust-basis-summary.md) | Markdown reviewer projection |
| [`junit-trust-basis.xml`](../assets/evidence-receipts-in-action/promptfoo/junit-trust-basis.xml) | JUnit CI projection |

Raw diff:

```json
{
  "schema": "assay.trust-basis.diff.v1",
  "summary": {
    "regressed_claims": 0,
    "improved_claims": 0,
    "removed_claims": 0,
    "added_claims": 0,
    "metadata_changes": 0,
    "unchanged_claim_count": 10,
    "has_regressions": false
  }
}
```

Markdown projection:

```markdown
## Trust Basis Gate

**Status:** OK
**Schema:** `assay.trust-basis.diff.v1`
**Claim identity:** `claim.id`

| Category | Count | Blocking |
| --- | ---: | --- |
| Regressed claims | 0 | yes |
| Removed claims | 0 | yes |
| Unchanged claims | 10 | no |
```

JUnit projection:

```xml
<testsuite name="assay.trust-basis.diff" tests="0" failures="0" errors="0" skipped="0" time="0">
  <system-out>regressed=0 removed=0 improved=0 added=0 metadata=0 unchanged=10</system-out>
</testsuite>
```

In this non-regression path, the JUnit projection is a bounded summary
artifact, not a full per-claim test listing. The released Harness recipes also
include regression-fixture cases for seeing a failing JUnit projection.

## Run the Released Recipe

The full recipes live in Assay Harness and should stay there. This page links
to the released recipe docs instead of re-embedding the whole pipeline.

- [Promptfoo receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/PROMPTFOO_RECEIPT_PIPELINE.md)
- [OpenFeature decision receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/OPENFEATURE_DECISION_RECEIPT_PIPELINE.md)
- [CycloneDX ML-BOM model receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/CYCLONEDX_MLBOM_MODEL_RECEIPT_PIPELINE.md)

The proof artifacts on this page were generated with the released Assay
`v3.9.1` binary and the released Harness `v0.3.2` gate/report surface. The
Harness manual compatibility workflow can also be used as an example proof
run, but it is not the primary source of truth because workflow artifacts are
run-scoped and retention-bound.

## Copyable GitHub Actions Proof

If you want the smallest workflow-native version, use this as a repo-local
proof over the checked-in D1 assets. It does not call upstream APIs and does
not regenerate the recipes. It verifies the released proof bundles, writes a
small job summary, and uploads the canonical/projection artifacts for review.

```yaml
name: evidence-receipts-proof

on:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  proof:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download Assay v3.9.1
        shell: bash
        run: |
          set -euo pipefail
          base="https://github.com/Rul1an/assay/releases/download/v3.9.1"
          curl -fsSLO "$base/assay-v3.9.1-x86_64-unknown-linux-gnu.tar.gz"
          curl -fsSLO "$base/assay-v3.9.1-x86_64-unknown-linux-gnu.tar.gz.sha256"
          sha256sum -c assay-v3.9.1-x86_64-unknown-linux-gnu.tar.gz.sha256
          tar -xzf assay-v3.9.1-x86_64-unknown-linux-gnu.tar.gz
          ./assay-v3.9.1-x86_64-unknown-linux-gnu/assay --version

      - name: Verify checked-in proof bundles
        shell: bash
        run: |
          set -euo pipefail
          ASSAY="./assay-v3.9.1-x86_64-unknown-linux-gnu/assay"
          for family in promptfoo openfeature cyclonedx; do
            "$ASSAY" evidence verify "docs/assets/evidence-receipts-in-action/$family/evidence.tar.gz"
          done

      - name: Write proof summary
        shell: bash
        run: |
          {
            echo "## Evidence Receipts Proof"
            echo
            echo "| Family | Trust Basis claim | Canonical artifacts | Projections |"
            echo "| --- | --- | --- | --- |"
            echo "| Promptfoo | \`external_eval_receipt_boundary_visible\` | bundle, trust-basis.json, diff JSON | Markdown, JUnit |"
            echo "| OpenFeature | \`external_decision_receipt_boundary_visible\` | bundle, trust-basis.json, diff JSON | Markdown, JUnit |"
            echo "| CycloneDX ML-BOM | \`external_inventory_receipt_boundary_visible\` | bundle, trust-basis.json, diff JSON | Markdown, JUnit |"
          } >> "$GITHUB_STEP_SUMMARY"

      - name: Upload proof artifacts
        uses: actions/upload-artifact@v4
        with:
          name: evidence-receipts-proof
          path: |
            docs/assets/evidence-receipts-in-action/manifest.json
            docs/assets/evidence-receipts-in-action/**/evidence.tar.gz
            docs/assets/evidence-receipts-in-action/**/trust-basis.json
            docs/assets/evidence-receipts-in-action/**/trust-basis.diff.json
            docs/assets/evidence-receipts-in-action/**/trust-basis-summary.md
            docs/assets/evidence-receipts-in-action/**/junit-trust-basis.xml
```

This is a proof wrapper, not a required integration path. The full runnable
family recipes stay in Assay Harness.

## Boundary

This is a downstream evidence pattern, not an upstream integration claim.

Assay does not replace Promptfoo, OpenFeature, or CycloneDX. It reduces
selected bounded outputs from those systems into portable evidence receipts and
then compiles claim-level artifacts above them.

This page does not claim model correctness, flag correctness, BOM completeness,
upstream endorsement, compliance certification, or official integration. It
also does not add SARIF to the Trust Basis gate/report path. SARIF remains a
good fit when there are real file/line anchors; the Trust Basis proof here is
claim-level, so raw JSON plus Markdown and JUnit projections are the right
shape.

## References

- [Assay `v3.9.1` release](https://github.com/Rul1an/assay/releases/tag/v3.9.1)
- [Assay Harness `v0.3.2` release](https://github.com/Rul1an/Assay-Harness/releases/tag/v0.3.2)
- [Receipt family matrix](https://github.com/Rul1an/assay/blob/v3.9.1/docs/reference/receipt-family-matrix.json)
- [Receipt schema registry](https://github.com/Rul1an/assay/tree/v3.9.1/docs/reference/receipt-schemas)
- [Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory](EVIDENCE-RECEIPTS-FOR-AI-OUTCOMES-RUNTIME-DECISIONS-MODEL-INVENTORY.md)
- [Example Harness release-binary proof run](https://github.com/Rul1an/Assay-Harness/actions/runs/25131209377)
