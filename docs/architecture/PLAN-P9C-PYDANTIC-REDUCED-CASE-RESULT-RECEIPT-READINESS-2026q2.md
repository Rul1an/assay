# PLAN — P9c Pydantic Reduced Case-Result Receipt Readiness Freeze (2026 Q2)

- **Date:** 2026-05-03
- **Owner:** Evidence / Product
- **Status:** Proposed
- **Scope:** Freeze the readiness boundary for possible importer-only support
  for the P9b Pydantic reduced case-result artifact. This is not importer
  implementation, not a public receipt family, not a Trust Basis claim, not a
  Harness recipe, and not a Pydantic integration claim.

## 1. Decision

P9b proved the right reduction direction: one reduced case-result artifact
derived from `EvaluationReport.cases[]`.

P9c should not reopen that slice as a broader `ReportCase` import. It should
freeze the rules that must be true before any later importer-only work starts.

The important boundary is:

```text
EvaluationReport.cases[] is discovery input only.
The reduced case-result artifact is the possible import unit.
ReportCase itself is not the contract unit.
```

The importer unit is not fixed by the public docs alone. It remains contingent
on live shape inspection continuing to support a reduced artifact that is
smaller than `ReportCase`.

That is the lane. Klein maar stevig.

## 2. Current State

The sample in `examples/pydantic-ai-eval-report-evidence/` now:

- uses `pydantic-evals==1.89.1`;
- derives one reduced artifact from one `EvaluationReport.cases[]` entry;
- keeps `case_name`, bounded assertion/score result values, and export
  timestamp;
- rejects broad `ReportCase` fields such as raw task inputs,
  `expected_output`, model output, trace IDs, span IDs, report summaries, and
  Logfire context;
- emits sample-only placeholder envelopes, not Assay Evidence Contract receipt
  events.

That is strong enough for readiness work, not yet strong enough for a public
receipt-family story.

## 3. Non-Goals

P9c does not add:

- a new Assay Evidence Contract event type;
- a receipt schema registry row;
- a Trust Basis claim;
- a Trust Card row;
- a Harness recipe;
- a public receipt family;
- a Pydantic runtime, Logfire, trace, or span import;
- a full `EvaluationReport` import;
- a raw `ReportCase` import;
- model-correctness, evaluator-correctness, or upstream-runtime truth.

Any later importer-only support belongs in a separate P9d slice.

## 4. Reduced Artifact Boundary

The only acceptable future importer candidate is the P9b reduced artifact:

```json
{
  "schema": "pydantic-evals.report-case-result.export.v1",
  "framework": "pydantic_evals",
  "surface": "evaluation_report.cases.case_result",
  "case_name": "case-hello",
  "results": [
    {
      "kind": "assertion",
      "evaluator_name": "EqualsExpected",
      "passed": true
    },
    {
      "kind": "score",
      "evaluator_name": "ExactScorePoints",
      "score": 1.0
    }
  ],
  "timestamp": "2026-05-02T08:00:00Z"
}
```

This shape is a reduced downstream artifact. It is not a dumped upstream
`ReportCase` object.

The `surface` value is an Assay-side reduced artifact identifier. It is not an
upstream Pydantic field name and does not claim that Pydantic publishes a
surface with that exact identifier.

## 5. Identity Rules

Required bounded identity:

- `case_name`

For P9c readiness, `case_name` is the only docs-backed bounded identity field.

Optional only if naturally present in a future live-inspected shape:

- `case_id_ref`

Do not synthesize `case_id_ref` from:

- trace IDs;
- span IDs;
- report ordering;
- file positions;
- hashed prompts;
- hashed completions;
- task input or expected-output material.

If the only stable case identity is `case_name`, Pydantic remains a
`case_name`-identified lane for v1.

## 6. Candidate Result Fields

These fields remain live-inspection-backed candidate receipt fields pending
readiness verification:

- `results[].evaluator_name`
- `results[].passed`
- `results[].score`
- `results[].reason`

P9b live inspection backs assertion and score result names/values for the
current sample version. P9c must keep that evidence status explicit: these are
not assumed to be docs-hard across every future `pydantic_evals` version.
P9c freezes that distinction rather than promoting these fields to stable
public reporting-doc truth.

`reason` is optional and may only be included when naturally present as a
bounded, non-empty string. Do not derive it from prompt, completion, expected
output, model output, trace payload, or evaluator internals.

## 7. Forbidden Fields

The reduced artifact remains malformed if it carries:

- `inputs`
- `input`
- `expected_output`
- `output`
- `metadata`
- `experiment_metadata`
- `trace_id`
- `span_id`
- `trace_url`
- `logfire_url`
- raw prompts
- raw completions
- report-wide summaries
- analyses
- evaluator implementation/config bodies

If one of these fields seems necessary to make the artifact reviewable, stop.
The correct outcome is either another recut or keeping Pydantic sample-only.

## 8. Claim And Harness Posture

P9c keeps Pydantic outside the Trust Basis claim surface.

No claim name is reserved in code or schema for Pydantic in P9c. If a future
claim is ever proposed, it must first define:

- exact claim id;
- exact predicate;
- negative examples;
- Trust Card impact;
- fixture behavior;
- Harness posture;
- compatibility notes for existing consumers.

Harness should not change for P9c. There is no gate/report recipe because no
Pydantic receipt family is claim-visible.

## 9. P9d Readiness Bar

A later P9d importer-only slice may start only if P9c can prove:

- the reduced artifact shape remains smaller than `ReportCase`;
- `case_name` is the required v1 identity;
- `case_id_ref` remains absent unless naturally live-backed;
- assertion pass/fail and scalar score fields are bounded and live-backed;
- malformed fixtures reject broad report, trace, input, expected-output, and
  model-output fields;
- the candidate importer can add standard receipt provenance without importing
  broader Pydantic report context;
- no Trust Basis claim, Trust Card row, or Harness recipe is required to make
  importer-only support useful.

If any of those fail, do not implement P9d. Houd hem dan netjes sample-only.

## 10. Acceptance Criteria

P9c is complete when:

- docs state that `EvaluationReport.cases[]` is discovery input only;
- docs state that the reduced case-result artifact, not `ReportCase`, is the
  possible import unit;
- `case_name` is the only docs-backed v1 identity and `case_id_ref` is
  optional only when naturally live-backed;
- candidate result fields are labeled as live-inspection-backed and
  readiness-bound rather than docs-hard;
- forbidden broad `ReportCase`, trace, Logfire, prompt, completion, input,
  expected-output, and model-output fields remain explicit;
- P9d readiness requirements are recorded before importer work starts;
- Harness is explicitly recorded as unchanged;
- release notes call this a readiness freeze, not feature expansion.

## 11. Short Verdict

Pydantic stays the best next lane, but only through the reduced case-result
boundary. In that boundary, `case_name` is the only docs-backed v1 identity;
`case_id_ref` stays out unless a later live inspection naturally exposes it.

P9c freezes that boundary. P9d may implement importer-only support later, but
only if the reduced artifact remains genuinely smaller than `ReportCase` and
does not pull in report, trace, Logfire, prompt, completion, or model-output
truth.
