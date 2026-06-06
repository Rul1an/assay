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

- Attestation binds who-said-it and the content; it does not upgrade observed support
  (proven in the attested-observed work) and provides no trust root or transparency
  log.

## References

- `assay-evidence/src/mandate/signing.rs`
- ADR-034 (contract seam)
