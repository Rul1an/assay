# assay evidence

Manage Assay evidence bundles and external evidence imports.

---

## Synopsis

```bash
assay evidence <COMMAND> [OPTIONS]
```

---

## OpenFeature Details Import

Import bounded OpenFeature boolean `EvaluationDetails` artifacts into a
verifiable Assay evidence bundle:

```bash
assay evidence import openfeature-details \
  --input openfeature-details.jsonl \
  --bundle-out openfeature-decision-receipts.tar.gz \
  --source-artifact-ref openfeature-details.jsonl
```

The importer is intentionally strict in v1:

- input must be JSONL with one bounded `EvaluationDetails` artifact per row
- each row must use `openfeature.evaluation-details.export.v1`
- each row must represent `target_kind = feature_flag`
- `result.value` must be boolean
- `result.reason` is a bounded string, not an Assay-owned enum
- provider config, evaluation context, targeting keys, rules, metadata,
  `error_message`, and full provider state are excluded

The importer first computes `source_artifact_digest` over the full JSONL file,
then parses and reduces decision details. Receipts stay small while still
binding back to the exact source artifact bytes.

The receipt is a decision-boundary artifact. It does not mean the flag decision
was correct, the application behavior was safe, the provider was correct, or
the targeting rules were imported as Assay truth.

The output bundle can be verified with:

```bash
assay evidence verify openfeature-decision-receipts.tar.gz
```

The same bundle can feed the Trust Basis compiler:

```bash
assay trust-basis generate openfeature-decision-receipts.tar.gz --out openfeature.trust-basis.json
```

P41 does not add a Trust Basis claim yet. The first OpenFeature compiler slice
proves the receipt bundle is bundleable, verifiable, and readable by the Trust
Basis path. Decision-specific Trust Basis claims are a later compatibility
decision.

Use `--import-time <RFC3339>` for deterministic fixture generation.

### Options

| Option | Description |
|--------|-------------|
| `--input <PATH>` | OpenFeature EvaluationDetails JSONL artifact file |
| `--bundle-out <PATH>` | Output Assay evidence bundle path |
| `--source-artifact-ref <REF>` | Reviewer-safe source artifact reference stored in receipts |
| `--run-id <ID>` | Assay import run id used for receipt provenance and event ids |
| `--import-time <RFC3339>` | Deterministic import timestamp override |

---

## Promptfoo JSONL Import

Import Promptfoo CLI JSONL assertion component results into a verifiable Assay
evidence bundle:

```bash
assay evidence import promptfoo-jsonl \
  --input results.jsonl \
  --bundle-out promptfoo-evidence.tar.gz \
  --source-artifact-ref results.jsonl
```

The importer is intentionally strict in v1:

- input must be Promptfoo CLI JSONL rows
- each row must carry `gradingResult.componentResults[]`
- each component must be an `equals` assertion result
- component scores must be binary (`0` or `1`)
- raw prompt, output, expected value, vars, and full JSONL rows are excluded

The importer first computes `source_artifact_digest` over the full JSONL file,
then parses and reduces assertion components. That two-pass flow is intentional:
receipts stay small while still binding back to the exact source artifact bytes.

`result.reason` is optional and bounded. For v1, failure reasons are omitted
when they would leak raw compared values. Passing reasons are included only
when they remain short and reviewer-safe.

The output bundle can be verified with:

```bash
assay evidence verify promptfoo-evidence.tar.gz
```

The same bundle can feed the Trust Basis compiler:

```bash
assay trust-basis generate promptfoo-evidence.tar.gz --out promptfoo.trust-basis.json
```

This proves the imported receipts are bundleable, verifiable, and readable by
the Trust Basis path. Trust Basis now emits
`external_eval_receipt_boundary_visible` when the supported Promptfoo receipt
shape is present. That claim means the bounded receipt boundary is visible; it
does not mean the Promptfoo eval run passed, the model output was correct, or
the raw Promptfoo payload is imported as Assay truth.

Use `--import-time <RFC3339>` for deterministic fixture generation.

To compare the resulting Trust Basis artifact against another run, use
[`assay trust-basis diff`](./trust-basis.md).

### Options

| Option | Description |
|--------|-------------|
| `--input <PATH>` | Promptfoo CLI JSONL output file |
| `--bundle-out <PATH>` | Output Assay evidence bundle path |
| `--source-artifact-ref <REF>` | Reviewer-safe source artifact reference stored in receipts |
| `--run-id <ID>` | Assay import run id used for receipt provenance and event ids |
| `--import-time <RFC3339>` | Deterministic import timestamp override |

---

## See Also

- [Evidence Contract v1](../../spec/EVIDENCE-CONTRACT-v1.md)
- [Trust Basis CLI](./trust-basis.md)
- [OpenFeature EvaluationDetails evidence example](../../../examples/openfeature-evaluation-details-evidence/README.md)
- [Promptfoo assertion grading-result example](../../../examples/promptfoo-assertion-grading-result-evidence/README.md)
- [From Promptfoo JSONL to Evidence Receipts](../../notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md)
- [P41 OpenFeature decision receipt import plan](../../architecture/PLAN-P41-OPENFEATURE-EVALUATION-DETAILS-DECISION-RECEIPT-IMPORT-2026q2.md)
- [P31 Promptfoo receipt import plan](../../architecture/PLAN-P31-PROMPTFOO-JSONL-COMPONENT-RESULT-RECEIPT-IMPORT-2026q2.md)
