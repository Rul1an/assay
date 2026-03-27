# SPLIT-MOVE-MAP — Wave48 Step1 — `registry/trust.rs`

## Goal

Freeze the split boundaries for `crates/assay-registry/src/trust.rs` before any mechanical
module moves.

## Planned Step2 layout

- `crates/assay-registry/src/trust.rs`
- `crates/assay-registry/src/trust_next/mod.rs`
- `crates/assay-registry/src/trust_next/decode.rs`
- `crates/assay-registry/src/trust_next/pinned.rs`
- `crates/assay-registry/src/trust_next/manifest.rs`
- `crates/assay-registry/src/trust_next/cache.rs`
- `crates/assay-registry/src/trust_next/access.rs`

## Mapping preview

- `trust.rs` keeps the stable routing surface for `TrustStore`, `KeyMetadata`, and public store accessors.
- `decode.rs` is the planned home for decode/validation helpers:
  - `decode_verifying_key`
  - `decode_public_key_bytes`
- `pinned.rs` is the planned home for pinned-root parsing and insertion helpers:
  - `parse_pinned_roots_json_impl`
  - `load_production_roots_impl`
  - `insert_pinned_key`
- `manifest.rs` is the planned home for manifest ingest and trust-rotation helpers extracted from:
  - `TrustStore::add_from_manifest`
- `cache.rs` is the planned home for refresh/cache helpers extracted from:
  - `TrustStore::needs_refresh`
  - `TrustStore::clear_cached_keys`
  - `TrustStore::empty_inner`
- `access.rs` is the planned home for access/query helpers extracted from:
  - `TrustStore::get_key_inner`
  - `TrustStore::is_trusted`
  - `TrustStore::list_keys`
  - `TrustStore::get_metadata`

## Frozen behavior boundaries

- identical pinned-root loading behavior
- identical key-id verification behavior
- identical revoked/expired-key handling
- identical cache/refresh behavior
- identical sync/async key lookup behavior
- no drift in resolver or verification trust lookup contracts
- no edits under `crates/assay-registry/tests/**` in Step2

## Test anchors to keep fixed in Step1

- `trust::tests::test_with_production_roots_loads_embedded_roots`
- `trust::tests::test_add_from_manifest`
- `trust::tests::test_pinned_key_not_overwritten`
- `trust::tests::test_needs_refresh`
- `trust::tests::test_trust_rotation_revoke_old_key`
- `trust::tests::test_trust_rotation_pinned_root_survives_revocation`
- `trust::tests::test_trust_rotation_key_expires_after_added`
- `resolver_accepts_signed_pack_with_embedded_production_root`
