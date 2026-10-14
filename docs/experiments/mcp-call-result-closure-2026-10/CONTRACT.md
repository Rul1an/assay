# Registered-action closure: small contract and twelve regression specifications

14 October 2026. Normalized checker contract, implemented by `closure_check.py`.
`CASES.json` contains the original twelve synthetic specifications. The MCP adapter
is a narrower mapping: its `supported` outcome means Assay-declared call-result
terminality only, and it cannot produce `contradicted`. See README.md.

## Question and boundary

Claim kind `all_selected_actions_terminal`: for the explicit selected registrations
in one session, a terminal action observation was available by `cutoff_seq`.
This is evidence-supported closure, not business success, process termination,
absence of hidden calls or honesty. The selected population must accompany every
result: never abbreviate a subset result to “all work finished”.

Profile `registered-action-closure.v0` consumes already-normalized synthetic
observations. It does NOT parse arbitrary MCP, natural-language logs, or disensor.
An adapter must bind native events to this profile; naming a normalized
state does not prove that the source justifies it. Unknown profiles refuse.
A review-cycle `declared_absence` has a different scope; producer claims are retained
but cannot be substituted for the explicit closure claim or completion evidence.

## Inputs (CASES.json is the literal fixture specification)

- `claim`: kind above, nonempty string session, positive integer cutoff_seq, explicit
  nonempty selection of `{action_id, attempt}`. Each expands to the full identity
  `(claim.session, action_id, attempt)`. action_id is a nonempty string, attempt a
  positive integer (booleans excluded). No coercion, inferred IDs, first/last match
  or “any successful retry”. There is only one selector form in v0.
- `context`: independently accepted test-controller premise: session,
  trace_sha256, source_class, continuity, coverage_through_seq, allowed_channels,
  observed_channels. It is not a producer-supplied completeness assertion or an
  authenticated trust root. False accepted premises cannot be detected from the
  same input bytes. All fixtures assume a trusted local supervisor, not isolation.
- `trace_utf8`: exact UTF-8 JSONL bytes supplied as a JSON string, final LF included.
  trace_sha256 is lowercase hexadecimal SHA-256 over those bytes, no reserialization.
  Rows have strictly increasing positive integer seq, session, action_id, attempt,
  kind and kind-specific fields. seq is observation order, NOT a wall clock or the
  physical time the action finished. Cutoff is an inclusive boundary in that order.
- `blobs`: digest -> exact UTF-8 text. An observation references evidence_sha256;
  the retained JSON object must match its seq, session, action_id, attempt and state.
  Each supplied blob must match its digest. Missing required bytes remain unknown;
  conflicting retained bytes refuse. Every supplied blob, including unreferenced
  blobs, is digest- and shape-validated; failure refuses under the table below.
  Hash the supplied blob string as UTF-8 verbatim, including whitespace, key order
  and any trailing LF. Never reserialize before hashing. A trailing LF is allowed
  but not required. Hash identity does not authenticate execution.

Kinds: `register`; `transport` with state accepted/timeout/cancel_requested;
`observation` with state completed/failed/pending; `producer_claim` with
claim_scope and declared_absence. No additional terminal states in this first
profile. Failed is terminal without business success. Other tokens/shapes refuse
as unsupported or invalid input rather than guessing their meaning. Duplicate JSON
members, invalid numeric types, unknown fields and malformed inputs refuse.

### Closed field shapes

All fields below are required; none are optional and extra fields refuse. `text`
means nonempty string; `uint` means integer >= 0, `positive` integer >= 1 (both
exclude booleans); `digest` means exactly 64 lowercase hexadecimal characters.
Lists contain unique elements unless duplication is specifically refused below.

| Object | Exact fields and domains |
|---|---|
| Input | profile: text; claim: object below; context: object below; trace_utf8: string; blobs: object mapping digest keys to strings |
| Claim | kind: `all_selected_actions_terminal`; session: text; cutoff_seq: positive; selection: nonempty list of selection objects |
| Selection entry | action_id: text; attempt: positive |
| Context | session: text; trace_sha256: digest; source_class: text; continuity: boolean; coverage_through_seq: uint; allowed_channels and observed_channels: lists of unique text values (empty permitted structurally) |
| Every row | seq: positive; session: text; action_id: text; attempt: positive; kind: one of the four kinds below |
| register | Only the common row fields |
| transport | Common fields plus state: `accepted`, `timeout`, or `cancel_requested` |
| observation | Common fields plus state: `completed`, `failed`, or `pending`; evidence_sha256: digest |
| producer_claim | Common fields plus claim_scope: `review_cycle`; declared_absence: boolean |
| Blob JSON object | seq: positive; session: text; action_id: text; attempt: positive; state: `completed`, `failed`, or `pending` |

Thus evidence_sha256 is forbidden on other row kinds. This small profile supports
only review_cycle producer claims; it deliberately does not infer other scopes.
A trace is zero or more JSON objects each followed by LF, with no blank records.

A full identity has exactly one registration. A selected registration must be at
or before cutoff. Every non-register row must reference an earlier registration
with the same identity. Repeated selected identities, missing registrations,
duplicate registrations and session disagreement refuse. Separate attempts are
separate identities; their results cannot close one another. The twelve fixtures
have one selected identity each; retry, subset and multi-action behavior is specified
but is not covered by these examples.

## Decision rules and reasons

1. Validate the entire representation and bindings before any conclusion. Use
   only the ordered refusal table below to select the primary reason; this rule
   introduces no separate ordering.
2. Observation evidence is admissible only under the accepted `local-supervisor`
   source premise with retained matching bytes. Another/unknown source cannot
   establish closure or contradiction. Preserve its values, but conclude
   `not_established` / `source_unaccepted` unless step 1 already refuses.
3. A retained pending observation for a **selected full identity**, **at seq == cutoff_seq**, is a counterexample:
   `contradicted` / `pending_at_cutoff`. An earlier pending observation does not
   establish the state at cutoff. This positive counterexample does not require
   complete log coverage. It does require the binding/source checks above.
   Pending observations for unselected identities do not contradict the selected claim.
4. Support requires continuity true, coverage_through_seq >= cutoff_seq, nonempty
   allowed_channels contained in observed_channels, and a retained completed or
   failed observation at or before cutoff for every selected registration.
   A terminal state is final for that attempt. A subsequent pending observation
   or a second terminal observation for that same attempt is an unsupported
   conflicting history: `no_claim` / `invalid_input`, not last-wins.
5. Without a counterexample, any of continuity false, coverage_through_seq below
   cutoff, empty allowed_channels, or an allowed channel absent from observed_channels gives `not_established` /
   `coverage_incomplete`; missing referenced in-bound observation bytes give
   `not_established` / `evidence_missing`; otherwise missing in-bound terminal
   observation gives `not_established` / `terminal_not_observed`. Transport
   accepted/timeout/cancel_requested, producer claims and observations after cutoff
   never supply a terminal observation. In-bound means seq <= cutoff_seq, for a
   selected full identity. Missing bytes for a pending-at-cutoff observation cannot
   contradict: they yield evidence_missing if coverage is sufficient. Missing bytes
   for an after-cutoff or unselected observation do not trigger evidence_missing
   for this claim. Late observations remain visible.
6. Otherwise: `supported` / `all_selected_terminal`. Preserve native states and
   reasons alongside this result; never project failed into business success.

Refusals are evaluated first, over the entire supplied input, including unselected
and after-cutoff rows. The first applicable row of this table determines the primary
no_claim reason. Evaluate later rows only when their required fields are well-typed.

| Priority / reason | Conditions |
|---|---|
| 1 invalid_input | Malformed JSON/JSONL, duplicate JSON members, missing/extra fields, wrong field type/range/encoding, malformed digest syntax, non-increasing seq, duplicate channel entries, empty selection; selected registration after cutoff; non-register row without an earlier same-identity registration; selected identity without registration only when claim, context and row sessions agree; a pending or second terminal observation after a terminal for the same identity |
| 2 unsupported_profile | Well-typed unknown profile, claim kind, row kind, per-kind state, blob state or claim_scope token |
| 3 binding_mismatch | Trace or supplied blob digest mismatch; session disagreement between claim/context/rows; any retained blob seq/session/action_id/attempt/state differing from its referencing observation |
| 4 ambiguous_selection | Repeated selected full identity or repeated registration of the same full identity |

Unknown row kinds are unsupported_profile, not missing-kind-specific-field errors;
validate their common fields first and do not infer a schema for their other fields.
Unknown state tokens are unsupported_profile, not a type error. Missing referenced
blob bytes alone are not a refusal. All independently detectable refusal reasons are
retained even though only the priority reason is primary.

After refusal validation, source admissibility is a mandatory gate: a source other
than local-supervisor returns not_established/source_unaccepted before considering
any pending counterexample. Only admissible observations enter the remaining tiers:
contradicted > not_established > supported. Within that remaining not_established
tier: coverage_incomplete > evidence_missing > terminal_not_observed. Structural
contradictory histories therefore preempt counterexamples; unaccepted sources cannot
contradict. Report all implicated identities and native observations; primary reason
selection never permits erasing others. This precedence is specified, not empirically
tested by the single-action cases.

## The twelve cases

| ID | Controlled difference | Expected outcome / primary reason |
|---|---|---|
| RC01 | Registered and completed before cutoff | supported / all_selected_terminal |
| RC02 | Registered and failed before cutoff | supported / all_selected_terminal; native failed retained |
| RC03 | Only accepted | not_established / terminal_not_observed |
| RC04 | Only timeout | not_established / terminal_not_observed |
| RC05 | Only cancel_requested | not_established / terminal_not_observed |
| RC06 | Completed observation after cutoff | not_established / terminal_not_observed |
| RC07 | RC01 observations, continuity false | not_established / coverage_incomplete |
| RC08 | Completion references absent bytes | not_established / evidence_missing |
| RC09 | Claim names different session | no_claim / binding_mismatch |
| RC10 | Context pins different trace hash | no_claim / binding_mismatch |
| RC11 | Same full identity registered twice | no_claim / ambiguous_selection |
| RC12 | Explicit pending at cutoff, plus review-cycle absence claim | contradicted / pending_at_cutoff |

Expectations are hand-specified. They are not generated by a checker or copied from
external expected answers. No foreign corpus/code is imported. Twelve examples are a deliberately small starting set, not
exhaustive coverage or a mutation score. Empty selections, malformed input, orphan
responses, earlier pending, multiple identities, typed-ID adapters and conflicting
histories are not covered by these twelve examples. The accompanying 29-method
checker suite and 21-case adapter suite extend them; see README.md for measured scope.
