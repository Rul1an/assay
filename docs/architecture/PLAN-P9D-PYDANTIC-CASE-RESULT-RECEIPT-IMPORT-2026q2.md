# PLAN — P9d Pydantic Case-Result Receipt Import (2026 Q2)

- **Date:** 2026-05-03
- **Owner:** Evidence / Product
- **Status:** Implemented
- **Scope:** Add importer-only support for the P9b/P9c reduced Pydantic Evals
  case-result artifact. This is not a Trust Basis claim, not a Trust Card row,
  not a Harness recipe, not a public receipt family, and not a Pydantic
  integration claim.

## 1. Decision

P9d adds the smallest compiler-path support for Pydantic Evals:

```text
pydantic-evals reduced case-result JSONL
  -> assay evidence import pydantic-case-result
  -> assay.receipt.pydantic.case_result.v1 receipt events
  -> verifiable evidence bundle
```

The import unit is the reduced artifact frozen by P9c. `ReportCase` remains
discovery input only; it is not the receipt contract.

## 2. Boundary

P9d imports:

- `case_name` as the only docs-backed v1 identity;
- bounded assertion result pass/fail values;
- bounded scalar score values;
- bounded evaluator names;
- optional bounded reason strings;
- optional `source_case_name` / `source_ref` as non-identity provenance aids;
- source artifact digest/provenance added by Assay.

P9d excludes:

- raw `ReportCase`;
- full `EvaluationReport`;
- task inputs;
- expected outputs;
- model outputs;
- report metadata;
- experiment metadata;
- trace IDs;
- span IDs;
- Logfire payloads;
- prompts;
- completions;
- analyses;
- failures bodies;
- evaluator implementation/config internals.

## 3. Non-Goals

P9d does not add:

- a Trust Basis claim;
- a Trust Card row;
- Harness gate/report behavior;
- a public Pydantic receipt family;
- model-correctness truth;
- evaluator-correctness truth;
- upstream runtime truth;
- Logfire, OpenTelemetry, trace, or span import.

## 4. Claim Posture

`assay.receipt.pydantic.case_result.v1` remains importer-only with
`trust_basis_claim: null`.

Any later claim-visible work must be a separate readiness/claim slice. It must
define the exact claim id, predicate, negative examples, Trust Card impact,
fixtures, Harness posture, and compatibility story before implementation.

## 5. Acceptance

P9d is complete when:

- the CLI imports valid reduced Pydantic case-result JSONL into verifiable
  bundles;
- the schema registry lists both input and receipt schemas as experimental and
  importer-only;
- the receipt family matrix keeps the lane in `importer_only_receipts`;
- tests prove Pydantic receipts do not mutate current eval, decision, or
  inventory Trust Basis claims;
- malformed broad `ReportCase` fields fail closed;
- `case_id_ref` remains unsupported unless a future live-backed slice changes
  that boundary explicitly;
- Harness remains unchanged.

## 6. Short Verdict

P9d makes the Pydantic reduced case-result lane bundleable and inspectable. It
does not make Pydantic claim-visible.
