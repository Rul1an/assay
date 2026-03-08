# SPLIT-MOVE-MAP — Wave17 Step2 — `replay/bundle`

## Goal

Mechanically split `crates/assay-core/src/replay/bundle.rs` into focused modules with zero behavior change and stable public API.

## New layout

- `crates/assay-core/src/replay/bundle/mod.rs`
- `crates/assay-core/src/replay/bundle/io.rs`
- `crates/assay-core/src/replay/bundle/manifest.rs`
- `crates/assay-core/src/replay/bundle/verify.rs`
- `crates/assay-core/src/replay/bundle/paths.rs`
- `crates/assay-core/src/replay/bundle/tests.rs`

Legacy file removed:
- `crates/assay-core/src/replay/bundle.rs`

## Mapping table

- `BundleEntry`, `ReadBundle` + public surface wiring -> `mod.rs`.
- `write_bundle_tar_gz`, `read_bundle_tar_gz`, tar/gzip entry append helpers -> `io.rs`.
- `build_file_manifest`, `content_type_hint` -> `manifest.rs`.
- `bundle_digest` -> `verify.rs`.
- `paths::*` constants + `validate_entry_path` helper -> `paths.rs`.
- inline tests from legacy file -> `tests.rs`.

## Frozen behavior boundaries

- archive canonical layout and deterministic ordering unchanged.
- path validation policy unchanged (fail-closed + canonical prefixes).
- file-manifest hash/size/content-type mapping unchanged.
- bundle digest semantics unchanged (`sha256(written tar.gz bytes)`).
- bundle read policy unchanged (`manifest.json` required, duplicate paths rejected).

## Test relocation map

Moved test names unchanged (non-exhaustive key anchors):
- `write_bundle_minimal_roundtrip`
- `bundle_digest_equals_sha256_of_written_bytes`
- `verify_clean_bundle_passes` (external replay verify module anchor)
- `read_bundle_roundtrip`
- `entries_written_in_sorted_order`
- `validate_entry_path_*`
