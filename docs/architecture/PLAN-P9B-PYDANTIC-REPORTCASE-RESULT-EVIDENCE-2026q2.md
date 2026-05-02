# PLAN — P9b Pydantic Reduced Case-Result Evidence Recut (2026 Q2)

- **Date:** 2026-05-02
- **Owner:** Evidence / Product
- **Status:** Implemented sample recut; importer-only support remains future
- **Scope:** Recut the existing Pydantic Evals sample around one bounded
  case-result artifact derived from `EvaluationReport.cases[]`. `ReportCase`
  is a discovery input, not the implied v1 contract unit. This is
  evidence-seam hardening toward a possible importer-only lane, not importer
  work by default, not a public receipt family, not a Trust Basis claim, not a
  Harness recipe, and not a public integration story.

## 1. Decision

Pydantic Evals is the next best candidate after the released three-family
receipt line, but only if the lane stays smaller than the original P9
report-wrapper shape.

The next slice should be:

```text
P9b - recut Pydantic sample around one reduced case-result artifact derived
from EvaluationReport.cases[]
```

The slice should not promote Pydantic into the public three-family receipt
story. It should produce a harder, smaller candidate lane that can later be
considered for importer-only receipt support only if the live recut proves the
case-result surface can stay narrow.

## 2. Why Pydantic Next

Pydantic Evals has the strongest combination of:

- strategic visibility;
- a code-first evaluation surface;
- a public result object that can be serialized or stored;
- an existing Assay-side sample;
- an open maintainer-routed issue for the external-consumer seam question.

The public docs establish the broad seam:

- Pydantic Evals is code-first.
- Running an experiment returns an `EvaluationReport`.
- The reporting API exposes `EvaluationReport.cases` as the natural smaller
  unit inside the report wrapper.

That makes Pydantic a better next candidate than another deterministic score
object lane, while still allowing Assay to stay away from Logfire,
OpenTelemetry, span-based evaluation, prompts, completions, and runtime truth.

The same docs also make the main risk clear: `ReportCase` can carry fields
such as inputs, expected output, output, metadata, `trace_id`, and `span_id`.
P9b must therefore derive a smaller case-result artifact from the case entry
rather than treating `ReportCase` itself as the import contract.

References:

- [Pydantic Evals overview](https://pydantic.dev/docs/ai/evals/evals/)
- [Pydantic Evals reporting API](https://pydantic.dev/docs/ai/api/pydantic_evals/reporting/)
- [Upstream seam question: pydantic-ai#5016](https://github.com/pydantic/pydantic-ai/issues/5016)

## 3. Relation To P9

The original P9 plan chose an `EvaluationReport`-derived artifact because that
was the first obvious public result surface. That was a good discovery shape,
but it is too wide for the next implementation step.

P9b supersedes P9 for execution.

P9 remains useful historical discovery. P9b is the narrower product discipline:

- one reduced case-result artifact derived from a report case;
- one evaluation result bag if the live shape naturally exposes one;
- no report-wide summary;
- no analyses;
- no experiment metadata;
- no trace/span metadata;
- no Logfire or telemetry payload.

## 4. Why Not The Other Candidates First

### AutoEvals

AutoEvals `ExactMatch` is technically clean and remains the best fallback. The
returned score object is small, deterministic, and already sample-backed.

It is not the best next wedge because Promptfoo already carries the public eval
outcome receipt story. A second deterministic score-object lane risks feeling
like more of the same before it adds a meaningfully new product boundary.

### AgentEvals

AgentEvals strict-match is strategically interesting because trajectory
matching is agent-specific. It should remain the second serious candidate.

It is not first because the seam pulls harder toward raw trajectories,
LangChain / LangSmith semantics, and runtime path truth. That makes it more
ambitious and easier to over-widen than a Pydantic per-case result.

### Phoenix And LangWatch

Phoenix and LangWatch have useful live-backed samples, but their seams are
platform and trace adjacent. They should stay parked until Assay wants a
deliberate platform-evaluation import slice.

### Guardrails

Guardrails opens an interesting validation / safety-signal family, but it has
the highest overclaim risk. A reader can too easily interpret "validation
passed" as "safe" or "correct". It should not be the next candidate.

## 5. Stop Rule

If the recut cannot make one reduced case-result artifact reviewable without
importing report, trace, Logfire, prompt, completion, expected-output,
model-output, or broad `ReportCase` context, stop before importer work.

In that case, do not patch around the gap by adding broad context. Recut the
candidate again or leave Pydantic as a sample-only lane.

This is the important guardrail: possible importer-only support is acceptable;
wider-than-case-result is not.

## 6. P9b V1 Shape

P9b should target a reduced JSON artifact derived from one
`EvaluationReport.cases[]` entry.

This is a reduced import artifact, not a raw upstream object and not a full
report export. `ReportCase` is the discovery source from which the smaller
artifact is derived, not the artifact schema itself.

Recommended shape:

```json
{
  "schema": "pydantic-evals.report-case-result.export.v1",
  "framework": "pydantic_evals",
  "surface": "evaluation_report.cases.case_result",
  "case_name": "checkout tax valid",
  "results": [
    {
      "evaluator_name": "EqualsExpected",
      "kind": "assertion",
      "passed": true
    },
    {
      "evaluator_name": "ExactScorePoints",
      "kind": "score",
      "score": 1.0
    }
  ],
  "timestamp": "2026-05-02T08:00:00Z"
}
```

This is the implemented sample shape after live inspection against
`pydantic-evals==1.89.1`. The live dump exposes bounded assertion result
names/values and score result names/values inside each case. `reason` remains
optional and is only included when naturally present as a non-empty string.

## 7. Required, Preferred, Optional

Required in the reduced artifact:

- `schema = "pydantic-evals.report-case-result.export.v1"`;
- `framework = "pydantic_evals"`;
- `surface = "evaluation_report.cases.case_result"`;
- `case_name` as the docs-backed bounded case identity when available;
- one or more reduced result values when live inspection exposes bounded
  pass/fail assertions or scalar scores without importing raw output or
  expected output;
- `timestamp` from the reduced export step, not a claim that upstream exposes a
  stable per-case timestamp.

Preferred:

- `results[].evaluator_name`, live-backed from assertion/score result entries;
- explicit `results[].passed` for assertion results;
- explicit `results[].score` for scalar score results.

Optional:

- short `results[].reason` or feedback when naturally present;
- `case_id_ref` only if the inspected live shape naturally carries a bounded
  case identifier;
- bounded `source_ref` if the recut needs a non-sensitive local artifact label.

Do not synthesize `case_id_ref` from report bookkeeping, trace IDs, span IDs,
or hashed prompt/output payloads. If the only stable case identity is
`case_name`, use `case_name`.

Assay-side receipt provenance, if P9b later graduates to importer-only
support:

- `source_artifact_ref`;
- `source_artifact_digest`;
- `reducer_version`;
- `imported_at`.

## 8. Field Evidence Levels

P9b must label candidate fields by evidence level before implementation:

- **Docs-backed:** `EvaluationReport`, `EvaluationReport.cases`, and
  case-level names such as `case_name` / `source_case_name` where present in
  the public reporting API.
- **Live-backed in `pydantic-evals==1.89.1`:** evaluator identity, pass/fail
  projection, scalar score projection, and short reason/feedback when present
  on assertion/score result entries.
- **Downstream/export provenance:** reduced artifact timestamp, source artifact
  label, reducer version, import timestamp, and artifact digest if later
  importer-only support is drafted.

Any field that cannot be placed in one of those buckets should stay out of the
v1 reduced artifact.

## 9. Non-Identity And Excluded Fields

The following are never part of receipt identity in P9b v1:

- trace references;
- span references;
- Logfire URLs;
- report names;
- experiment names;
- report-wide summary statistics;
- analyses;
- evaluator implementation/config internals.

The following must stay out of the reduced artifact:

- prompts;
- completions;
- raw model outputs;
- expected outputs;
- task inputs;
- full `EvaluationReport`;
- `failures` bodies;
- `analyses`;
- `experiment_metadata`;
- OpenTelemetry or Logfire payloads;
- span-based evaluator traces.

## 10. Execution Phases

### Phase A - Inspect The Current Upstream Shape

- Re-run the tiny local `Dataset(...).evaluate_sync(...)` generator against the
  current package versions.
- Inspect `EvaluationReport.cases[]` and any per-evaluation result structures.
- Record which fields are docs-backed, type-backed, or live-capture-backed.

### Phase B - Recut Fixtures

- Replace the report-wrapper fixture with one reduced per-case artifact.
- Keep one passing case, one failing case, and one malformed case.
- Preserve fixed timestamps for deterministic fixture output.
- Keep old P9 report-wrapper fixtures only if explicitly labeled historical,
  or remove them if they create ambiguity.

### Phase C - Harden The Mapper

- Map one reduced case-result artifact into an Assay-shaped placeholder
  envelope.
- Reject report-wide summaries, analyses, trace/span payloads, prompt/output
  payloads, and inline model/completion bodies.
- Add tests or fixture checks proving malformed broad payloads are rejected.

### Phase D - Decide Whether To Draft Importer-Only Receipt Support

Only after Phase A-C are clean, decide whether a later P9c should add an
importer-only receipt event.

P9b itself should not add a Trust Basis claim, Harness recipe, public note, or
claim-visible receipt family.

## 11. Acceptance Criteria

P9b is ready when:

- the sample centers on one reduced case-result artifact derived from
  `EvaluationReport.cases[]`, not a raw `ReportCase` object and not a full
  `EvaluationReport`;
- fixtures prove positive, negative, and malformed paths;
- the README says which fields are docs-backed versus live-capture-backed;
- `case_name` is treated as the docs-backed case identity, while `case_id_ref`
  is used only if live inspection naturally exposes it;
- evaluator identity, pass/fail, scalar score, and reason/feedback are derived
  only from live-backed assertion/score result entries;
- trace/span/Logfire fields are excluded rather than merely marked optional;
- raw prompts, completions, model outputs, task inputs, expected outputs,
  analyses, and report-wide summaries are rejected or absent;
- no Trust Basis claim or Harness recipe is added;
- no public communication is made beyond repo-internal docs/PR text.

## 12. Communication Rule

No public post for P9b.

The only acceptable external follow-up is a small update on the existing
Pydantic issue if one of these happens:

- a maintainer asks for the current Assay-side shape;
- the upstream issue risks stale closure and needs a factual keep-alive;
- a recut exposes a concrete field-level ambiguity that maintainers can answer.

Use "downstream reduced artifact" language. Do not say integration, support,
partnership, official, or Trust Basis family.

## 13. Short Verdict

Pydantic is next, but only as a reduced case-result artifact derived from
`EvaluationReport.cases[]`. P9b proves the sample recut; possible importer-only
support remains a later decision.

AutoEvals remains the clean fallback. AgentEvals remains the more ambitious
second-choice. Phoenix, LangWatch, and Guardrails stay parked until the product
needs a deliberate platform/safety-signal lane.
