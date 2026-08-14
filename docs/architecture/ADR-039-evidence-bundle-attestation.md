# ADR-039: Evidence Bundle as in-toto / SCITT Attestation

## Status
Proposed (June 2026) — trigger-gated.

Depends on ADR-034 (contract seam).

## Context

The evidence bundle has a manifest, Merkle root, and content-addressed events, but is
not emitted as an attestation statement. DSSE signing already exists, scoped to the
mandate subsystem (`assay-evidence/src/mandate/signing.rs`), and CI emits SLSA
build-provenance for the binary. The bundle itself cannot be anchored or verified
offline as a portable claim. As of 2026 the in-toto Attestation Framework (ITE-6) is
the common envelope that Sigstore and SLSA already use, and SCITT continues through
the IETF, synergising with RATS and WIMSE.

> Correction (2026-08-14): the shipped `run_root` is SHA-256 over newline-delimited
> event content-hash strings, with a trailing newline, in event sequence order —
> not a tree root, and not `event_id` bytes. The historical wording above describes
> the model used at the time and is not a claim about the shipped evidence format.

## Decision

Emit the bundle manifest and the coverage/claim verdict as an in-toto v1 Statement
under a named custom predicate type (mirroring how SLSA defines its predicate),
wrapped in a DSSE envelope, reusing the mandate signing path. Keep the anchor
pluggable (SCITT statement or OpenTimestamps); do not build a transparency log or
trust root. The per-fact claim-state (basis) is a first-class predicate field.

## Gate

Publish the predicate type and ship the emitter only once an independent consumer
evaluates or consumes it. Until that trigger, this ADR records the decision and the
shape; it is intentionally not built, to avoid freezing a predicate no one consumes.

## Consequences

- An Assay coverage/claim verdict becomes a portable attestation other systems can
  anchor and verify offline, composable under a SCITT statement or content-addressed
  record.
- Adds a predicate schema to version and keep stable once published.

## Best-practice basis (2026)

- in-toto ITE-6 as the common envelope; SLSA provenance is an in-toto attestation
  with a named predicate; SCITT in the IETF with RATS + WIMSE.

## Non-claims

- Attestation binds who-said-it and the semantic event chain; it does not upgrade observed
  support (proven in the attested-observed work) and provides no trust root or transparency
  log.
- **The subject does not identify the artifact.** `statement_from_manifest` uses
  `manifest.run_root` as the subject digest, and `run_root` chains the per-event content
  hashes. Those cover exactly `{specversion, type, datacontenttype, subject?, data}` and nothing
  else, so a re-export at a different time keeps the same chain. Everything outside that set is
  excluded by construction -- stream identity, `time`, trace context, producer and policy
  metadata, and the privacy flags; `crypto/id.rs` carries the enumerated list, and it is the one
  place worth reading, because a second copy of it is a second thing to keep true. The cost of
  that property is that a bundle whose
  `run_id`, event ids, producer, timestamps and PII flags are all rewritten *consistently* has a
  bit-identical `run_root` and satisfies the same attestation. in-toto assumes the opposite:
  "Subjects are assumed to be immutable" and subjects are "matched purely by digest". So a
  consumer who reads a satisfied attestation as proof of *which* bundle they hold is relying on
  a property this subject does not have. The repair is to separate the two roles, not to widen
  the digest -- widening it would break the deterministic re-export the profile makes normative
  (`docs/profiles/privileged-mcp-action/v0.md:124`) and `crypto/id.rs` implements.
  Tracked as its own ADR on the programme ledger (#1866).

## References

- `assay-evidence/src/mandate/signing.rs`
- ADR-034 (contract seam)
