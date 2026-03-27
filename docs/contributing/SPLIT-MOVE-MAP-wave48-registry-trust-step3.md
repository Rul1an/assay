# SPLIT-MOVE-MAP — Wave48 Step3 — `registry/trust.rs` Closure

## Shipped layout

Wave48 Step2 is now the shipped split shape on `main`:
- `crates/assay-registry/src/trust.rs`
- `crates/assay-registry/src/trust_next/mod.rs`
- `crates/assay-registry/src/trust_next/decode.rs`
- `crates/assay-registry/src/trust_next/pinned.rs`
- `crates/assay-registry/src/trust_next/manifest.rs`
- `crates/assay-registry/src/trust_next/cache.rs`
- `crates/assay-registry/src/trust_next/access.rs`

## Ownership freeze

- `crates/assay-registry/src/trust.rs`
  remains the stable facade for `TrustStore`, `KeyMetadata`, and top-level routing.
- `crates/assay-registry/src/trust_next/decode.rs`
  remains the decode and key-material parsing boundary.
- `crates/assay-registry/src/trust_next/pinned.rs`
  remains the production-root and pinned-key loading boundary.
- `crates/assay-registry/src/trust_next/manifest.rs`
  remains the manifest ingest and trust-rotation boundary.
- `crates/assay-registry/src/trust_next/cache.rs`
  remains the refresh and cache-state boundary.
- `crates/assay-registry/src/trust_next/access.rs`
  remains the sync/async read access and metadata boundary.

## Allowed follow-up after closure

- documentation updates only
- reviewer-gate tightening only
- future internal visibility tightening only if it requires a separate code wave

## Explicitly deferred

- new module cuts
- trust-store semantic cleanup
- pinned-root, manifest, or cache behavior changes
- resolver behavior or verification coupling changes
- validation or result-shape changes
- registry contract or public surface changes
