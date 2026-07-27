# ADR-044: The attestation subject is the artifact, not the semantic chain

- Status: Proposed
- Date: 2026-07-27
- Supersedes: none
- Amends: ADR-039 (evidence bundle attestation)

## Context

`run_root` serves two purposes that cannot both be served by one value, and the collision is
currently resolved in favour of the wrong one.

Its first purpose is semantic equivalence. It chains the per-event `content_hash`, and that hash
covers `{specversion, type, datacontenttype, subject?, data}` and nothing else. Excluding `time`,
stream identity and producer metadata is deliberate and correct: it is what allows the same events,
repackaged later by a different producer, to keep the same hash. That property is normative in
`docs/profiles/privileged-mcp-action/v0.md`, it is what `conformance/privileged-mcp-action-v0`
depends on, and it is precisely what issue #1840 asks third parties to reproduce.

Its second purpose is to be the in-toto subject digest. `statement_from_manifest` puts
`manifest.run_root` in `subject[0].digest.sha256`, so the DSSE signature is bound to that value.

The in-toto v1 Statement specification is explicit about what a subject digest is:

> Subjects are assumed to be _immutable_, i.e. the artifacts identified by the subject SHOULD NOT
> change.

> Subject artifacts are matched purely by digest, regardless of content type.

`run_root` is not immutable with respect to the artifact, by construction. Two bundles that differ
in `run_id`, producer, timestamps, the PII flag and `source` share it.

### Reproduction

Taking `fuzz/corpus/bundle_reader/valid-three-events` and rewriting the stream identity
consistently — every event's `assayrunid` and `id`, the manifest's `run_id`, the producer, the PII
flag and the timestamps — then re-pinning only the `events.ndjson` digest:

```
run_id:   run_verifier_property_0001 -> forged_run_identity_000100
producer: assay-evidence-property-test -> forged-producer
assaypii: false -> true
time:     2023-11-14 -> 2099-01-01
run_root: sha256:6af106bd... UNCHANGED
=> VERIFIED OK
```

The same holds for `source` alone, on the shipped fixture `tests/fixtures/evidence/test-bundle.tar.gz`:
rewriting it across all five events and re-pinning only the file digest verifies clean with
`run_root` bit-identical (`sha256:7a47bb25...` before and after).

So a valid DSSE signature over a genuine bundle matches a forged bundle **by digest**, which is the
only thing an in-toto verifier is told to match on. The consequence is not that the signature is
weak; it is that the signature answers a different question than the one a consumer asks. It
attests "these events, in this order, carry this semantic content". A consumer reading
`subject.digest` reasonably believes it attests "this artifact".

Nothing in this analysis says the exclusion list is wrong. Widening `content_hash` would break
deterministic re-export, break the clean-room conformance pack, and re-merge the two roles rather
than separating them. The defect is the reuse, not the digest.

### What the chain does not carry

The complete inventory is enforced by `crates/assay-evidence/tests/content_hash_field_inventory.rs`
rather than restated here, because every hand-written copy of it has gone stale. The load-bearing
consequence for this decision: `source` — the CloudEvents field naming the system that produced
the stream — is outside the chain, alongside `run_id`, `seq`, producer identity and version,
`git_sha`, `policy_id`, `time`, trace context, and the `contains_pii` / `contains_secrets` flags.

An attestation over `run_root` therefore does not establish **who produced the events, when, under
which policy, or whether they were classified as containing personal data**.

## Decision

**1. The subject digest is the digest of the bundle bytes.**

`subject[0].digest.sha256` becomes the SHA-256 of the `.tar.gz` a consumer holds. That value is
already a pinned determinism input — `docs/spec/EVIDENCE-CONTRACT-v1.md:125` lists "SHA-256 of the
entire compressed tar.gz" among the three exact inputs — so this introduces no new computation, only
a new use of one the format already fixes. It is *not* `bundle_id`, which equals `run_root` and is
therefore a semantic value; `subject[0].name` keeps the bundle identifier for human reference.

**2. Provenance moves into the predicate.**

`run_id`, `run_root`, producer identity and version, `git_sha`, `event_count` and the run's time
window are predicate fields. The predicate sits inside the DSSE envelope, so these are signed; they
are simply not the matching key. This is the same split SLSA and cosign use: subject identifies the
bytes, predicate says what is claimed about them.

**3. `run_root` MUST NOT appear as a second entry in the same `DigestSet`.**

This is normative and not stylistic. The in-toto `DigestSet` rule is:

> Two DigestSets SHOULD be considered matching if ANY acceptable field matches.

A `DigestSet` carrying both `sha256` (artifact) and a custom `run_root` field would match a forged
bundle on the second key, restoring the exact defect this ADR removes, through a field added to
document it. `run_root` belongs in the predicate, where matching semantics do not apply.

**4. The two roles are named wherever either is used.**

`run_root` is the *semantic equivalence digest*: equal iff two bundles carry the same events, in the
same order, with the same content. The bundle digest is the *artifact digest*: equal iff the bytes
are identical. A document or comment that says an attestation "binds the bundle content" without
saying which of the two is meant is not making a checkable statement.

## Consequences

Existing attestations do not verify against the new subject rule and must be re-issued. There is no
migration path that preserves them, because their subject digest does not identify their artifact —
that is the finding.

Consumers matching on `subject.digest` gain artifact identity and lose the ability to recognise a
re-export as "the same evidence". That recognition is still available, from `run_root` in the
predicate, and it becomes an explicit act rather than an accident of which digest was in the
matching position.

The clean-room conformance pack is unaffected. `pack_format.py` contains no reference to `run_root`
at all: `rewrite_bundle_stream_identity` rewrites `assayrunid`, the event `id`s and the manifest
`run_id`, re-pins the `events.ndjson` digest, and leaves the chain root untouched by never
addressing it. That is the property #1840 relies on. Under this decision the same rewriting
produces a bundle with an unchanged semantic digest and a *different* artifact digest — which is
exactly the distinction the pack exists to demonstrate, now expressible instead of collapsed.

Verifier behaviour does not change. `bundle_id == run_root` remains the enforced manifest
invariant (check 14, ADR-043 lineage); this ADR governs what the attestation binds, not what the
verifier accepts.

## Open

- Whether the predicate keeps a `semantic_equivalence` block naming `run_root` explicitly, or
  whether `run_root` sits at the predicate top level. Preference: a named block, so a consumer
  cannot mistake it for a second artifact digest.
- The `corpus_digest_method` string in the conformance manifest is ambiguous between the prefixed
  and bare-hex readings of "vector sha256" (the implementation uses the prefixed form). Same class
  of defect — a digest procedure that two honest implementers read differently — and it should be
  disambiguated in the same pass.

## Implementation evidence

Measured on `7ba8f066`, worktree `assay-adr044`:

- consistent stream-identity rewrite verifies clean with `run_root` unchanged (probe against
  `verify_bundle_with_limits`, default limits);
- `source`-only rewrite on `tests/fixtures/evidence/test-bundle.tar.gz` verifies clean, `run_root`
  bit-identical, independently reproduced;
- `statement_from_manifest` (`crates/assay-evidence/src/attestation.rs`) places `manifest.run_root`
  in the subject digest;
- in-toto v1 Statement text quoted above read from the specification repository, not from a
  secondary description.
