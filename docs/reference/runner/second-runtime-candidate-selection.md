# Assay-Runner Second Runtime Candidate Selection

> Internal Phase 2B selection note. This page records evaluations of concrete
> runtime candidates against the entry plan in
> [`second-runtime-plan.md`](second-runtime-plan.md). It is not a runtime
> selection record yet, not a dependency proposal, and not a fixture design.

**Status:** skeleton. This note does not select a candidate yet. Each
candidate section below is a placeholder. The evaluation form is fixed; the
content is added one candidate per iteration.

This page implements the deliverable defined by
<https://github.com/Rul1an/assay/issues/1295>.

## Evaluation Discipline

Each candidate is evaluated independently against the seven Candidate
Requirements from
[`second-runtime-plan.md` § Candidate Requirements](second-runtime-plan.md#candidate-requirements).
Evaluations are conservative: when public evidence is thin, the outcome is
`insufficient evidence`, not an optimistic `qualifies`.

A candidate is not selected by this note simply by appearing here. Selection
requires `qualifies` against all seven requirements **and** an explicit
selection statement in the Selection Outcome section below.

## Outcome Vocabulary

Every requirement evaluation uses exactly one of:

| Outcome | Meaning |
|---|---|
| `qualifies` | Public evidence clearly shows the requirement is met. The evaluation row cites the specific evidence. |
| `does not qualify` | Public evidence shows the requirement is not met. The line is closed for this candidate. |
| `insufficient evidence` | The requirement might be met, but current public evidence is not strong enough to say so. The candidate is paused, not rejected. |

`insufficient evidence` is a first-class outcome. It must not be promoted to
`qualifies` because a reviewer feels the gap is small.

A candidate's overall outcome equals the **lowest** outcome across its seven
requirement rows:

- all seven `qualifies` → candidate overall `qualifies`
- any `does not qualify` → candidate overall `does not qualify`
- otherwise (any `insufficient evidence`, no `does not qualify`) → candidate
  overall `insufficient evidence`

## Stable Identity — Level-3 Checklist

The Stable identity requirement uses the level-3 interpretation defined in
[`#1295` Stable identity — required interpretation](https://github.com/Rul1an/assay/issues/1295).
A candidate satisfies Stable identity only if at least one field meets **all
three** conditions:

| Condition | Definition | Acceptable evidence |
|---|---|---|
| **runtime-generated** | The value is produced by the runtime/SDK, not by the fixture or adapter | Public docs or typed API contract showing runtime generation |
| **binding-intended** | The field is meant to bind the same tool call or action across event boundaries | Public docs describing the field's binding purpose, or a stable SDK event schema explicitly modeling it |
| **run-window unique** | The value is unique within the run window for one action | Public docs or schema confirming uniqueness scope |

Source code may support the evaluation, but a `qualifies` outcome should
prefer public docs, typed API contracts, or stable SDK event schemas over
incidental implementation details. An internal variable name found by source
archaeology does not establish binding-intent on its own.

Disqualifying identity signals (these never satisfy Stable identity):

- generic trace ids without tool-call binding semantics
- request, response, or message ids without binding-intent in the runtime
  contract
- fixture-injected ids (the fixture chose the value, not the runtime)
- ids that exist only by source archaeology, not in the public contract
- ids that are not stable across the same tool call's event boundaries

## Candidate Evaluation Form

Each candidate is recorded as one subsection under `## Candidates` below. The
subsection follows this fixed form. Reviewers should be able to read the
candidate row-by-row and reach the same overall outcome.

```markdown
### Candidate: <runtime name>

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 2 | Stable identity | qualifies / does not qualify / insufficient evidence | <see Stable Identity row below> |
| 3 | Comparable surface | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 4 | Deterministic dependency lock | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 5 | Linux/eBPF fit | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 6 | Small event shape | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 7 | Evidence boundary fit | qualifies / does not qualify / insufficient evidence | <link or quote> |

**Stable identity detail (only filled if row 2 is `qualifies`):**

- Field name: `<field>`
- Source: `<event type / API entrypoint>`
- runtime-generated evidence: <link or quote>
- binding-intended evidence: <link or quote>
- run-window unique evidence: <link or quote>

**Overall outcome:** `qualifies` / `does not qualify` / `insufficient evidence`

**Notes:** <optional, one or two sentences of context — no advocacy language>
```

## Candidates

*Placeholder. No candidate has been added yet. Each candidate evaluation lands
in its own PR following the Candidate Evaluation Form above.*

## Selection Outcome

*Filled in only after at least one candidate evaluation reaches `qualifies`.
A `does not qualify` or `insufficient evidence` outcome for all evaluated
candidates leaves this section as a placeholder stating "no candidate
currently qualifies"; the selection issue
[`#1295`](https://github.com/Rul1an/assay/issues/1295) remains open until a
qualifying candidate exists.*

## Non-Goals For This Note

This selection note does not:

- propose runtime dependencies
- propose fixture code
- introduce call-id-less correlation fallback
- propose a narrower delegated gate for the second runtime
- propose cross-runtime capability-diff against S5
- promote level-1 or level-2 identity signals to satisfy Stable identity
- promote an `insufficient evidence` outcome to `qualifies`

Each of those is a separate decision that follows only after this note
records a clear `qualifies` outcome.

## How To Add A Candidate

1. Open a docs-only PR titled along the lines of
   `[codex] evaluate <runtime> against second runtime entry plan`.
2. Add one subsection under `## Candidates` using the form above.
3. Cite public docs, typed API contracts, or stable SDK event schemas in the
   Evidence cells. Source code citations are acceptable as supporting
   evidence but not as the sole basis for a `qualifies` row, especially for
   row 2 (Stable identity).
4. Do not alter the Evaluation Discipline, Outcome Vocabulary, Stable
   Identity checklist, or Candidate Evaluation Form sections except through a
   separate contract-discipline PR.
5. Set the candidate's overall outcome by the lowest row outcome rule above.
6. Only when an overall outcome is `qualifies` AND the Selection Outcome
   section is updated to name this candidate as the selected second runtime,
   may the first fixture implementation PR be opened.

## References

- [Runner second runtime Phase 2B plan](second-runtime-plan.md)
- [Runner CI lane contract](ci-lanes.md)
- [Runner acceptance fixture v0 contract](fixtures-v0.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Assay-Runner boundary and extraction map](boundary-map.md)
- Selection issue: <https://github.com/Rul1an/assay/issues/1295>
