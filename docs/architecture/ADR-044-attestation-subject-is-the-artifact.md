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

The conclusion this reproduction supports is narrower than "the forged bundle matches", and the
narrower statement is the more serious one.

A conforming in-toto consumer hashes the `.tar.gz` it holds and compares that to
`subject[0].digest.sha256`. The stored value is `run_root`, which is not the SHA-256 of any archive.
So under conforming matching **neither the genuine bundle nor the forged one matches** — the subject
digest is not merely wrong about which artifact it names, it names no artifact at all.

Assay's own consumer does not perform that comparison. `verify_envelope` checks the DSSE payload
type, verifies the Ed25519 signature over the PAE, asserts the Statement `_type`, and returns the
Statement. It never touches `subject`. So today:

- a conforming consumer rejects every bundle, genuine or forged, for failing to match;
- Assay's consumer accepts both, because it matches nothing;
- the forged bundle "matches" only under a non-standard comparison of `run_root` against `run_root`.

The forgery therefore survives not because it matches, but because **nothing is matched**. Fixing
the producer side alone does not establish artifact identity: a correct subject digest that no
consumer compares is still an unchecked field. The decision below has to cover both ends.

The signature is not weak. It attests "these events, in this order, carry this semantic content" —
a true and useful statement. A consumer reading `subject.digest` reasonably believes it attests
"this artifact", and no part of the current path makes that belief checkable.

Nothing in this analysis says the exclusion list is wrong. Widening `content_hash` would break
deterministic re-export, break the clean-room conformance pack, and re-merge the two roles rather
than separating them. The defect is the reuse, not the digest.

### What the chain does not carry

The complete inventory is not restated here, because every hand-written copy of it has gone stale.
It is enforced by `crates/assay-evidence/tests/content_hash_field_inventory.rs`, which **does not
yet exist on `main`** — it lands with PR #1886, and this ADR takes a hard dependency on that merge.
Until then the citation names an intended mechanism, not an available one. The load-bearing
consequence for this decision: `source` — the CloudEvents field naming the system that produced
the stream — is outside the chain, alongside `run_id`, `seq`, producer identity and version,
`git_sha`, `policy_id`, `time`, trace context, and the `contains_pii` / `contains_secrets` flags.

An attestation over `run_root` therefore does not establish **who produced the events, when, under
which policy, or whether they were classified as containing personal data**.

## Decision

**1. The subject digest is the digest of the completed archive bytes.**

`subject[0].digest.sha256` becomes the SHA-256 of the finished `.tar.gz` — the exact byte sequence a
consumer receives, gzip trailer included. `subject[0].name` keeps the bundle identifier for human
reference.

This defines no new digest, and the distinction matters: the format already fixes this value as a
determinism input (`docs/spec/EVIDENCE-CONTRACT-v1.md:125`, "SHA-256 of the entire compressed
tar.gz"). It does require **new runtime data flow**. `statement_from_manifest` receives only a
`Manifest` today and cannot see the container, so the archive digest has to be computed where the
archive is finalised and threaded into statement construction. It is *not* `bundle_id`, which equals
`run_root` and is therefore a semantic value.

**2. Consumers MUST match the subject against the bytes they hold.**

A correct subject digest that nobody compares is still an unchecked field, and that is the state the
reproduction above actually documents. `verify_envelope` currently returns the Statement without
touching `subject`. It MUST, given the archive bytes, recompute their SHA-256 and reject a mismatch
before returning; a caller that has no bytes to offer MUST get a distinct result meaning
*signature-verified, artifact-unmatched*, never a plain success.

Producer-side changes alone do not establish artifact identity. Both ends move or neither does.

**3. The predicate is versioned, with a schema, and carries provenance in named blocks.**

The current builder accepts arbitrary JSON under `evidence-bundle/v0`, so a legacy statement and a
conforming one are indistinguishable. This decision introduces a new predicate type — a fresh
version, not a relaxation of the old one — whose required fields are specified rather than
conventional:

```json
"semantic_equivalence": {
  "algorithm": "assay-run-root-v1",
  "value": "sha256:..."
}
```

`run_root` lives there and nowhere else. Required provenance fields: `run_id`, producer name and
version, `git_sha`, `event_count`, and the run's time window. Values derivable from the matched
bundle MUST be cross-checked against it by the consumer; a predicate that disagrees with the artifact
it is attached to is a rejection, not a note.

The named block exists so a consumer cannot read the semantic digest as a second artifact digest.
That is the same confusion at one remove, and naming is what prevents it.

**4. Exactly one subject, and it identifies an artifact.**

Stronger than barring `run_root` from a `DigestSet`, and for the reason the upstream spec gives:
every `DigestSet` entry is required to identify an immutable artifact, so a semantic-equivalence
identifier is out of place anywhere in the subject array, not just alongside `sha256`.

The narrow rule remains true and is why the broad one is needed — the `DigestSet` rule is:

> Two DigestSets SHOULD be considered matching if ANY acceptable field matches.

A second key would match a forged bundle through the very field added to document the problem. But
barring one field invites the next one; requiring a single artifact subject closes the class.

**5. The two roles are named wherever either is used.**

`run_root` is the *semantic equivalence digest*. Equal values are intended to mean two bundles carry
the same ordered sequence of content-hash inputs — not that the events are equal objects: the
excluded fields may differ, which is the whole point of the exclusion. The archive digest is the
*artifact digest*: equal values are intended to mean identical bytes.

Both are identity relations under their chosen algorithm rather than mathematical biconditionals,
and SHA-256 collision resistance is the assumption underneath each. Stating them as `iff` would
overclaim in two directions at once — the first because equal roots permit different events, the
second because digest equality is computational, not logical.

A document or comment saying an attestation "binds the bundle content" without saying which of the
two is meant is not making a checkable statement.

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

## Resolved during drafting

Three questions were left open in the first draft and settled by the co-author rather than by the
author alone:

- **Predicate layout**: the named `semantic_equivalence` block, because it keeps semantic identity
  out of in-toto's artifact-matching namespace instead of relying on readers to know the difference.
- **Corpus digest wording**: `corpus_digest_method` is to state that the hash is taken over each
  stored `vectors[i].sha256` **string including the literal `sha256:` prefix**, each followed by LF,
  in manifest order. The implementation already does this; the sentence did not say so, and the
  bare-hex reading is the more natural one — a procedure two honest implementers read differently is
  the same defect class as this ADR's, one layer down.
- **Clean-room pack**: confirmed unaffected. It carries no DSSE attestations at all, and rewritten
  bundles keep `run_root` while their artifact bytes change — which is the distinction this decision
  makes expressible.

## Dependencies

- PR #1886 (`content_hash_field_inventory.rs`) must merge before this ADR is Accepted; the "what the
  chain does not carry" section cites a mechanism that does not exist on `main` until then.

## Implementation evidence

Measured on `7ba8f066`, worktree `assay-adr044`:

- consistent stream-identity rewrite verifies clean with `run_root` unchanged (probe against
  `verify_bundle_with_limits`, default limits);
- `source`-only rewrite on `tests/fixtures/evidence/test-bundle.tar.gz` verifies clean, `run_root`
  bit-identical, independently reproduced;
- `statement_from_manifest` (`crates/assay-evidence/src/attestation.rs`) places `manifest.run_root`
  in the subject digest;
- `verify_envelope` (same file) checks payload type, signature and Statement `_type`, then returns
  the Statement — it reads `subject` nowhere, which is what makes the missing consumer-side match a
  finding about the present rather than a hypothetical;
- in-toto v1 Statement text quoted above read from the specification repository, not from a
  secondary description.
