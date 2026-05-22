# Assay-Runner Cross-Runtime Diff Phase 2C Decisions

> Internal Phase 2C decision note. This document records the resolution of
> the central open question in
> [`cross-runtime-diff-plan.md`](cross-runtime-diff-plan.md) — which
> structural differences between two clean normalized runner evidence sets
> are runtime-implementation noise and which are capability-surface
> meaningful. It is not a contract, not a schema freeze, not a golden
> shape, and not implementation work. It unlocks the Phase 2C contract
> slice PR; it does not perform it.

This note resolves the decision gate tracked in
<https://github.com/Rul1an/assay/issues/1310>. It does not extend the
Phase 2C scope beyond what `cross-runtime-diff-plan.md` already records,
and it does not pre-approve any work outside the next contract slice PR.

## Chosen Combination

**A1 + B3 + C1.**

| Dimension | Choice | One-line meaning |
|---|---|---|
| A. Fixture file paths | **A1** | Normalize only the work-dir prefix to `<work>/`; keep fixture-local filenames as observed capability-surface values |
| B. Tool-call binding ids | **B3** | Binding ids are out of scope for cross-runtime comparability in v0; they remain required for within-runtime correlation only |
| C. SDK metadata | **C1** | SDK metadata is reported as side-band runtime provenance, not as `added`/`removed`/`unchanged` capability-surface values |

The combination is deliberately narrow. Broader cross-runtime semantics
require a separate post-v0 contract.

## Decision Statement

> v0 cross-runtime diff should remove obvious fixture plumbing, preserve
> observed capability-surface differences, and avoid derived
> cross-runtime identity semantics. Binding ids remain required for
> within-runtime correlation, but are not themselves cross-runtime
> comparable in v0.

## Decision A — Path projection (A1)

**Choice.** Apply work-dir prefix canonicalization only. Replace any
absolute path prefix that points at an acceptance-script `mktemp -d`
work directory with a single canonical `<work>/` placeholder. Do not
quotient fixture-local filenames.

**Why A1, not "both layers are noise".** Two layers of path noise sit
between the S5 and Gemini capability-surfaces:

- *Work-dir prefix* (`/tmp/assay-runner-openai-agents-kernel-policy/work/`
  vs `/tmp/assay-runner-gemini-google-genai-kernel-policy/work/`) — pure
  execution noise; each acceptance script picks its own `mktemp -d`
  template, and the prefix tells the reader nothing about what the
  agent did.
- *Fixture-local filename* (`openai-agents-input.txt` vs
  `gemini-input.txt`) — fixture-author choice. Quotienting these would
  silently claim semantic equality across runtimes, which v0 must not do.

A1 strips enough noise to make the diff usable without claiming any
hidden equivalence. Filenames remain visible in the surface; if two
fixtures happen to use the same filename, the v0 diff reports
`unchanged` purely syntactically and makes no semantic-equality claim
(see Non-Claims below).

**Operational rule.** The canonicalization rule is purely syntactic:
match a configured set of acceptance-script work-dir prefixes and
replace each with `<work>/`. The rule must not be per-runtime; it must
not encode adapter knowledge; it must not look at filename or content.

## Decision B — Binding-id semantics (B3)

**Choice.** Binding ids (`tool_call_id`) are **out of scope** for
cross-runtime comparability in v0. They remain load-bearing inside each
side: every input evidence set must still pass the within-runtime
`capability-diff-v0` clean-correlation rule, and binding-id absence
still produces `partial:correlation` or `failed` on the source side.

**Why B3, not "per-run identity tokens" (B1).** B1 would still surface
binding ids in the cross-runtime output as a vacuous always-disjoint
comparison (`tc_runner_policy_001` vs `ho0csecf`, reported as
`unchanged=[]`). B3 is contractually cleaner because:

1. It avoids a *false negative diff*. Reporting "binding ids differ"
   between S5 and Gemini is technically true but is not a
   capability-difference; surfacing it invites readers to treat
   per-run tokens as a semantic dimension.
2. It avoids a *new derived identity scheme*. B2 (derive a stable
   cross-runtime id from `bound_tool_name` + `bound_policy_decision`)
   would itself require a separate contract slice and would establish
   pseudo-identity semantics not present in `capability-diff-v0`.

**Operational rule.** Cross-runtime v0 must not report binding ids as
`added`, `removed`, `changed`, or `unchanged`. Binding ids remain
required for per-side verification only. A future post-v0 contract may
introduce a cross-runtime identity projection; v0 does not.

## Decision C — SDK metadata (C1)

**Choice.** SDK package name and SDK package version (and equivalent
runtime provenance fields) are reported as side-band runtime
provenance. They are visible in the diff output but do not participate
in the capability-surface `added`/`removed`/`unchanged` projection.

**Why C1.** SDK metadata is runtime-implementation, not capability:
S5's `@openai/agents 0.11.4` and Gemini's `google-genai 2.6.0` describe
which SDK observed the run, not what capability the agent exercised.
Treating it as `added`/`removed` would conflate observation provenance
with capability comparison.

**Operational rule.** SDK metadata may be shown as side-band runtime
provenance (e.g. an explicit `sdk_metadata` block listing both sides)
but does not participate in capability-surface comparison. It is
visible, useful for diagnostics, and not a capability claim.

## Non-Claims

This decision note does not:

- declare cross-runtime semantic equality of fixture-local filenames
  (if S5 and Gemini happen to both use `policy-input.txt`, the v0 diff
  may report it `unchanged` syntactically; that is not a claim that
  the underlying capability is the same)
- introduce a derived cross-runtime identifier scheme
- treat SDK metadata as capability equivalence
- define declared-capability semantics
- decide third-runtime behavior
- propose a new artifact category
- modify v0 artifact contracts, fixture v0 contracts, or
  `capability-diff-v0`
- pre-approve cross-runtime live LLM calls or cassette regeneration
- propose a delegated gate change, lane-check rule change, or CI lane
  addition

## What This Unlocks

The next discoverable step is the Phase 2C contract slice PR
(step 4 in `cross-runtime-diff-plan.md` § Suggested Slice Sequence).
That PR may proceed under this decision combination. It must:

- freeze `cross-runtime-diff-v0` either as a new section in
  `capability-diff-v0.md` or as a sibling document, whichever the
  contract author judges cleaner
- include a golden shape for the cross-runtime `S5 ↔ Gemini` case
- implement A1 path canonicalization as a syntactic prefix rule with
  no per-runtime knowledge
- treat binding ids per B3 — out of cross-runtime comparison entirely
- treat SDK metadata per C1 — side-band, not added/removed/unchanged
- preserve all `capability-diff-v0` preconditions (clean health,
  within-runtime stable binding identity, idempotent `diff(X, X)`)
- not introduce a new fixture, new delegated gate, or new lane-check
  rule

## What Remains Forbidden In The Contract Slice PR

- adding a new fixture (third-runtime work remains paused per
  `second-runtime-candidate-selection.md`)
- broadening A1 into filename-layer quotienting
- introducing a derived cross-runtime identifier scheme
- promoting SDK metadata into capability-surface comparison
- introducing a delegated `gates=all` requirement for cross-runtime
  diff
- regressing intra-runtime `diff(X, X)` idempotence from
  `capability-diff-v0`
- declaring acceptability semantics (`is this change OK?` remains
  policy/reviewer responsibility, not diff responsibility)
- modifying lane-check classifier rules

## Revisit Conditions

This decision note may need to be revisited (in a follow-up note, not
by silent edit) if any of these become true:

- a third runtime fixture lands and exposes a path noise pattern that
  A1's prefix-only canonicalization cannot reasonably classify
- a Phase 2C+ slice requires comparing binding identity across
  runtimes in a way B3 forbids
- SDK metadata grows fields that are clearly capability-bearing rather
  than provenance (would call C1 into question)

If revisited, the new combination is recorded in a new decisions note
that supersedes this one; this file remains as the historical record.

## References

- [Cross-runtime diff Phase 2C mini-plan](cross-runtime-diff-plan.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Runner capability-diff Phase 2B plan](capability-diff-plan.md)
- [Runner second runtime Phase 2B plan](second-runtime-plan.md)
- [Runner second runtime candidate selection](second-runtime-candidate-selection.md)
- Decision gate (resolved by this note): <https://github.com/Rul1an/assay/issues/1310>
