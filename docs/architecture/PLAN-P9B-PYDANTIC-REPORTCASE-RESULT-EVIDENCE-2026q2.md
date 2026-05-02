# PLAN - P9b Pydantic ReportCase Result Evidence Recut (2026 Q2)

- **Date:** 2026-05-02
- **Owner:** Evidence / Product
- **Status:** Proposed next execution slice
- **Scope:** Recut the existing Pydantic Evals sample around one bounded
  `EvaluationReport.cases[]` / `ReportCase`-derived result. This is
  evidence-seam hardening only: importer-only if it graduates, not a public
  receipt family, not a Trust Basis claim, not a Harness recipe, and not a
  public integration story.

## 1. Decision

Pydantic Evals is the next best candidate after the released three-family
receipt line, but only if the lane stays smaller than the original P9
report-wrapper shape.

The next slice should be:

```text
P9b - recut Pydantic sample around one ReportCase-derived result
```

The slice should not promote Pydantic into the public three-family receipt
story. It should produce a harder, smaller candidate lane that can later be
considered for importer-only receipt support.

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

- one report case;
- one evaluation result bag;
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

## 5. P9b V1 Shape

P9b should target a reduced JSON artifact derived from one
`EvaluationReport.cases[]` / `ReportCase` result.

This is a reduced import artifact, not a raw upstream object and not a full
report export.

Recommended shape:

```json
{
  "schema": "pydantic-evals.report-case-result.export.v1",
  "framework": "pydantic_evals",
  "surface": "evaluation_report.cases.report_case",
  "case_id_ref": "case:checkout-tax-valid",
  "case_name": "checkout tax valid",
  "evaluator_name": "Equals",
  "result": {
    "passed": true,
    "score": 1.0,
    "reason": "matched expected output"
  },
  "timestamp": "2026-05-02T08:00:00Z"
}
```

This example is illustrative. The implementation must verify the actual
current `ReportCase` / per-evaluation result shape before freezing fixture
fields.

## 6. Required, Preferred, Optional

Required in the reduced artifact:

- `schema = "pydantic-evals.report-case-result.export.v1"`;
- `framework = "pydantic_evals"`;
- `surface = "evaluation_report.cases.report_case"`;
- one bounded case identity field: `case_id_ref` or `case_name`;
- one bounded evaluator identity field when naturally exposed;
- one result value: pass/fail or scalar score;
- `timestamp` from the reduced export step, not a claim that upstream exposes a
  stable per-case timestamp.

Preferred:

- `case_id_ref`;
- `evaluator_name`;
- explicit `result.passed` when the upstream result is boolean;
- explicit `result.score` when the upstream result is scalar.

Optional:

- short `result.reason` or feedback when naturally present;
- `case_name` as reviewer-facing display text;
- bounded `source_ref` if the recut needs a non-sensitive local artifact label.

Assay-side receipt provenance, if P9b later graduates to importer-only
support:

- `source_artifact_ref`;
- `source_artifact_digest`;
- `reducer_version`;
- `imported_at`.

## 7. Non-Identity And Excluded Fields

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

## 8. Stop Rule

If the recut cannot make one `ReportCase` reviewable without importing report,
trace, Logfire, prompt, completion, or model-output context, stop before
importer work.

In that case, do not patch around the gap by adding broad context. Recut the
candidate again or leave Pydantic as a sample-only lane.

This is the important guardrail: importer-only is acceptable; wider-than-case
is not.

## 9. Execution Phases

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

## 10. Acceptance Criteria

P9b is ready when:

- the sample centers on one `ReportCase`-derived result, not a full
  `EvaluationReport`;
- fixtures prove positive, negative, and malformed paths;
- the README says which fields are docs-backed versus live-capture-backed;
- trace/span/Logfire fields are excluded rather than merely marked optional;
- raw prompts, completions, model outputs, task inputs, expected outputs,
  analyses, and report-wide summaries are rejected or absent;
- no Trust Basis claim or Harness recipe is added;
- no public communication is made beyond repo-internal docs/PR text.

## 11. Communication Rule

No public post for P9b.

The only acceptable external follow-up is a small update on the existing
Pydantic issue if one of these happens:

- a maintainer asks for the current Assay-side shape;
- the upstream issue risks stale closure and needs a factual keep-alive;
- a recut exposes a concrete field-level ambiguity that maintainers can answer.

Use "downstream reduced artifact" language. Do not say integration, support,
partnership, official, or Trust Basis family.

## 12. Short Verdict

Pydantic is next, but only as a ReportCase-bounded importer-only candidate.

AutoEvals remains the clean fallback. AgentEvals remains the more ambitious
second-choice. Phoenix, LangWatch, and Guardrails stay parked until the product
needs a deliberate platform/safety-signal lane.
