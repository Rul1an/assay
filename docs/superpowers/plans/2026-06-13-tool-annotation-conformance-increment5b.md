# Increment 5b — live wiring for `assay.tool_annotation_conformance.v0`

Follows Increment 5a (`2026-06-13-tool-annotation-conformance-increment5.md`), which shipped the
pure producer contract. 5b wires it into the enforcing proxy.

## Goal

Capture the declared MCP tool annotation hints from the observed `tools/list` snapshot, and emit one
`assay.tool_annotation_conformance.v0` record per enforced `tools/call`, beside the existing
`assay.enforcement_decision.v0` (verdict) and `assay.manifest_establish.v0` (journey) carriers. The
conformance carrier stays orthogonal: its outcome never changes the verdict and never gates the call
(an evidence-write failure can still fail closed before delivery, like the other carriers; see
record-write ordering below).

## Grounding and future hook

5b is grounded in the current MCP tools contract: `tools/list` (paginated), `tools/list_changed`, the
optional Tool `annotations` object, and the rule that clients must treat tool annotations as
untrusted unless they come from a trusted server. There is no trusted-server signing mechanism today,
so every annotation is untrusted and an independent declared-vs-observed comparison is always
meaningful.

Future hook: the July 28 release-candidate track is treated only as a future compatibility hook. It
may add adjacent task and server-discovery surfaces, but 5b deliberately sources only from the
current `tools/list` annotations snapshot. Tasks and `server/discover` are not source inputs for 5b;
they remain follow-up surfaces if that line lands.

## Single observation source

The proxy observer already retains the latest complete `tools/list` snapshot (`latest_complete`) and
answers the drift gate through `observed_tool_digest(tool_name)`, which reads that snapshot under the
supersede and ambiguity rules. 5b derives the declared annotations from the SAME effective snapshot,
so the conformance record can never read a different manifest view than the verdict did:

- a single observer helper returns, for a tool name, both the observed `tool_digest` and the raw
  declared `annotations`, from the effective `latest_complete`;
- because `latest_complete` is updated by the establish re-list (the same accumulation path), the
  conformance record is automatically based on the post-establish, re-decided observation, not the
  original pre-establish view.

`extract_declared_annotations` parses the raw `annotations` value into the four v0 hint fields.
Absent, null, non-object, or non-boolean hints all read as `None`: the carrier records what the
server actually declared, and 5a treats an absent hint as undeclared, never the MCP schema default.

## Observation basis (honest v0 extension)

Behavior classification is manifest-independent (it reads the tool name and arguments), so a call can
be classified even when the manifest was not completely observed. In that case the declared
annotations are not reliably observed, and running the 5a logic blind would emit `undeclared`,
falsely implying the server declared nothing when really nothing was observed.

5b therefore adds one append-only field to the v0 record, `observation_basis`:

- `complete` — the called tool was present in the effective complete manifest; the real per-tool
  `tool_digest` is recorded and the 5a conformance logic applies;
- `incomplete` — no complete manifest, an ambiguous manifest, or the tool absent from an otherwise
  complete manifest; `conformance` is forced to `inconclusive`, declared hints are all null, and
  `tool_digest` is null, with a non-claim that annotations were not observed.

This is an append-only shape change made while there is still no consumer; the 5a producer fixture
and its exact-key assertion are regenerated in the same change. `observation_basis` and `tool_digest`
move together, which also makes both the populated and null `tool_digest` forms appear in the
contract.

## Emission and record-write ordering

When `--tool-conformance-out <path>` is set, the proxy writes one NDJSON record per enforced
`tools/call`, after `assay.enforcement_decision.v0` and `assay.manifest_establish.v0`:

- on an allow, all carrier writes must succeed or the call becomes `proxy_failed` and is not
  forwarded — an allow is the decision to forward, never the delivery, so a failed conformance write
  must not let an unrecorded call through;
- on a deny, a conformance write failure is logged only and the deny stands; the carrier may still be
  emitted on a deny, where declared annotation versus observed behavior is useful forensics on a
  refused call.

## Non-correlation

The conformance outcome is computed at the `tools/call` boundary: the record contents never change
the verdict, and a mismatch never gates the call. A failure to write a requested record is a separate
evidence-integrity rule that can fail closed before delivery, the same rule the other carriers follow
(see record-write ordering). It covers the call, not any asynchronous task execution a server may
start. UI-driven tool calls route through the same call path and are covered without special
handling.

## Slices

- 5b-1: `extract_declared_annotations` and its tests (absent / null / non-object / non-boolean /
  partial hints). Pure, no wiring.
- 5b-2: the single-source observer helper (tool_digest plus annotations from the effective snapshot),
  the `--tool-conformance-out` flag, carrier emission with `observation_basis`, fail-closed evidence
  on allow and logged-only on deny, the producer-fixture regeneration, and a verb-sync guard test
  asserting every classifier verb maps to an observed behavior.
- 5c (Plimsoll): vendor the regenerated fixture, add a CLI acceptance test, and extend the combined
  acceptance fixture to three carriers, proving the conformance signal promotes to neither the
  verdict nor the establish journey.

## Tests (5b-2)

- complete manifest, annotation mismatch, allow verdict → `mismatched` record, verdict stays allow;
- incomplete manifest → `observation_basis: incomplete`, `inconclusive`, null `tool_digest`;
- flag off → no file written;
- allow with a conformance write failure → `proxy_failed`, not forwarded;
- deny with a conformance write failure → deny stands;
- verb-sync guard.

## Non-claims

- tool annotations are untrusted hints, read-only to compare, never to decide privilege or the
  verdict;
- a `consistent` record is not trust certification; a `mismatched` record is not a maliciousness
  verdict or an enforcement decision;
- observed behavior is the call classification, not verification of the upstream side effect;
- `observation_basis: incomplete` means the annotations were not observed, not that the server
  declared none;
- `idempotentHint` and `openWorldHint` are recorded but not assessed in v0.
