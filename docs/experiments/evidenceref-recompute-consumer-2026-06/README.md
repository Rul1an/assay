# Independent evidenceRef recomputation consumer

> Experimental vectors for independent `evidenceRef` recomputation. Not a stable Assay API.
> This is the recomputation and resolution layer only, not a trust, issuer, or effect verdict.

A small, fail-closed consumer for content-addressed `evidenceRef` references. It resolves a reference,
recomputes the digest from the referenced bytes under the declared canonicalization, checks the declared
schema, and renders one verdict, reaching a clean verdict only by recomputation. A reference runner plus
a separate independent reproducer, machine-readable vectors, reproducible from the bytes alone.

## Run

```bash
python3 evidenceref_consumer.py emit   > vectors.json   # regenerate the vector bytes
python3 evidenceref_consumer.py verify vectors.json      # reference: reproduce + measurement
python3 independent_consumer.py vectors.json             # independent: reproduce from bytes alone
pytest                                                   # 22 tests: matrix, invariants, interop
```

Over 17 vectors the consumer reaches a clean (`recomputed`) verdict exactly twice; every other verdict is
non-clean. `independent_consumer.py` re-derives every verdict from `vectors.json` with separate code that
shares no import with the reference runner (asserted by an AST test), so the set reproduces from the bytes
alone rather than from one implementation trusting another. Two canonicalization profiles are exercised,
`jcs-json-v1` (RFC 8785 JCS) and `cbor-deterministic-v1` (RFC 8949 section 4.2), so a reference resolves
through one shared `{digest, canonicalization, schema}` shape.

## Three claims

1. **Independent recomputation, fail closed.** A consumer can resolve a content-addressed `evidenceRef`
   and render one deterministic verdict from committed bytes, reaching clean only by recomputing the
   digest under the declared canonicalization and confirming a complete record under the declared schema.
   Digest-only, unresolvable, unsupported-profile, mismatched, and incomplete cases all fail closed.
2. **The producer envelope is not the verdict authority.** The producer's own state flag, schema hints,
   and embedded profile definitions are never inputs. Required fields, completeness rules, and
   canonicalization profile meanings are resolved by the consumer's own trusted registries, so a producer
   cannot declare a redacted field non-required or redefine what a profile name means.
3. **Reproducible by a second, independent implementation.** Every verdict re-derives from the published
   vectors by a separate reproducer that shares no code with the reference runner, across two distinct
   canonicalization profiles. Two implementations resolving the same reference by recomputation alone is
   the bar; one implementation verifying itself is not.

## Five non-claims

1. **Recomputation is not trust.** A clean verdict means the bytes match the content address under the
   declared canonicalization and schema. It does not mean the producer is honest, the issuer or signature
   is trusted, or the claimed effect occurred.
2. **A digest match is not claim sufficiency.** A digest match proves the projection bytes are intact; it
   does not prove the projection is complete enough to support the claim.
3. **This is the recomputation/resolution layer only.** Grounding the claim against an independent
   observation is a separate axis; verifying issuer or signature trust is another. Neither is in scope.
4. **No live infrastructure.** It operates on committed bytes only and queries no producer, registry, or
   attestation service.
5. **Bounded value space.** The two profiles cover the value space of these vectors (float-free JSON and
   the CBOR types used here); full profile coverage and any counts beyond this vector set are out of scope.

## Verdicts

Clean is reached only by `recomputed`. Every other verdict is non-clean, split into positive
disagreements (`digest_mismatch`, `canonicalization_mismatch`, `schema_mismatch`, `malformed_ref`) and
inconclusive states (`unresolvable_digest_only`, `unresolved_ref`, `unsupported_canonicalization`,
`redacted_projection_incomplete`). The machine-readable canonicalization profiles and schema registry,
the per-case body store, and each case's committed verdict all live in `vectors.json`.
