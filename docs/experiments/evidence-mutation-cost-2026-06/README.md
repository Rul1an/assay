# Evidence mutation-detection matrix and verification-cost curve

Signed and attested artifact lines are widening, but one question tends to stay underexposed: for a
signed evidence bundle, *which* post-hoc mutation classes are actually detected, by *which* verifier
check, and at *what cost*? This experiment measures exactly that, with a reproducible harness.

We measure which post-hoc mutation classes are detected by the verifier, and at what cost, under a
no-signing-key attacker model.

## What this is, and is not

This is a measurement, not a blanket security claim. Two things are reported:

1. A **mutation-detection matrix** for `assay-evidence` bundles, under threat model T: a party
   *without* the signing key mutates a bundle after the fact, while the bundle's run anchor
   (`run_root`, a Merkle root over event content hashes) is bound by an external signature the
   attacker cannot forge. Two layers are exercised:
   - the internal verifier (`verify_bundle_with_limits`), which catches blind tampering that breaks
     internal consistency, each with a specific error code;
   - the run anchor, which catches a *consistent rewrite* (events and manifest recomputed together)
     that the internal verifier alone would accept, because the content-addressed root changes.
2. A **verification + signing cost curve**: verify time as a function of bundle size, compressed
   size and gzip ratio, bytes per event, the Merkle inclusion-proof size (ceil(log2(N)) hashes), and
   DSSE sign and verify time over the run anchor.

It is tamper-evident, not tamper-proof. A host that holds both code execution and the signing key is
out of scope: it can simply re-sign, which is why production deployments anchor externally (a
transparency log or a timestamping authority). And there is one honest internal limitation the
matrix surfaces rather than hides: manifest metadata not referenced by a verifier check is not
individually hash-bound (the `manifest_meta_only` column). The event evidence (`events.ndjson`) and
the run anchor are always hash-checked; a stray flip in a cosmetic manifest field can pass the
internal verifier when a truncated read skips the gzip CRC trailer. The event evidence itself is
never accepted in a mutated form, which is what the gate enforces.

## Results

See [results/matrix.md](results/matrix.md) and [results/cost.md](results/cost.md), generated from
the JSON the harnesses emit. Headline numbers from a sample release run:

- Across every mutation class (447+ bitflips plus truncation, file injection, path traversal,
  absolute path, byte edits, drops, reorders, BOM, CRLF, duplicate entries), zero mutations changed
  the event evidence and still verified. Each class is rejected with a sensible verifier code, and
  the consistent rewrite is caught by the run anchor.
- Verification is linear in event count at roughly 16 ms per 1000 events (r² ~ 1.0). DSSE signing
  and verification over the run anchor are well under a tenth of a millisecond. Cost numbers are
  machine and workload dependent, so treat them as a shape and a method, not a universal constant.

## How to run

The gate runs in normal CI as part of the workspace test suite:

```bash
cargo test -p assay-sim --test e3_mutation_matrix       # mutation-detection gate (no bypass)
cargo test -p assay-evidence --test e3_verify_cost_curve # cost-path smoke
```

The full sweep plus JSON artifacts is produced by pointing `E3_OUT_DIR` at a directory (release is
recommended for representative timings):

```bash
E3_OUT_DIR=docs/experiments/evidence-mutation-cost-2026-06/results \
  cargo test -p assay-sim --release --test e3_mutation_matrix
E3_OUT_DIR=docs/experiments/evidence-mutation-cost-2026-06/results \
  cargo test -p assay-evidence --release --test e3_verify_cost_curve
python3 docs/experiments/evidence-mutation-cost-2026-06/aggregate.py
```

## Files

- `results/matrix.json`, `results/matrix.md` — the mutation-detection matrix (`assay.experiment.evidence_mutation_matrix.v0`).
- `results/cost.json`, `results/cost.bmf.json`, `results/cost.md` — the cost curve and its Bencher Metric Format export (`assay.experiment.evidence_verify_cost.v0`).
- `aggregate.py` — stdlib renderer from JSON to Markdown.

The harnesses live in `crates/assay-sim/tests/e3_mutation_matrix.rs` (mutation matrix, reusing the
sim mutators and the evidence verifier) and `crates/assay-evidence/tests/e3_verify_cost_curve.rs`
(cost curve and DSSE timing).
