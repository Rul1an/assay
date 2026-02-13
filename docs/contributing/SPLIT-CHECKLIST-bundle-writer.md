# Bundle writer split — checklist & freeze contracts

Status: Wave 1 / Step 1 (behavior freeze before mechanical split)

## Inventory snapshot (sanity, pre-split)

```bash
git rev-parse HEAD
# b702baefa7547a7ca6ad9ae5d4becc61ff38971c

wc -l crates/assay-evidence/src/bundle/writer.rs
# 1442 crates/assay-evidence/src/bundle/writer.rs

rg -n "pub struct BundleWriter|verify_bundle" crates/assay-evidence/src/bundle -S
# crates/assay-evidence/src/bundle/writer.rs:122:pub struct BundleWriter<W: Write> {
# crates/assay-evidence/src/bundle/writer.rs:384:pub fn verify_bundle<R: Read>(reader: R) -> Result<VerifyResult> {
# crates/assay-evidence/src/bundle/writer.rs:693:pub fn verify_bundle_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<VerifyResult> {
```

## Commit-1 scope lock

- This step contains only:
  - contract checklists
  - behavior-freeze tests
  - split boundary grep-gates
- This step explicitly excludes perf/alloc optimizations.

## Behavior-freeze contracts

- Byte determinism:
  - same normalized input events => byte-identical `tar.gz` bundle.
- Verify error shape:
  - unexpected file => `ErrorClass::Contract` + `ErrorCode::ContractUnexpectedFile`
  - path traversal => `ErrorClass::Security` + `ErrorCode::SecurityPathTraversal`
  - max events exceeded => `ErrorClass::Limits` + `ErrorCode::LimitTotalEvents`
  - file size limit exceeded => `ErrorClass::Limits` + `ErrorCode::LimitFileSize`
- Manifest/events order is strict (`manifest.json` first, then `events.ndjson`).

## Boundary contract for split target

Target layout:

```text
bundle/writer/
  mod.rs
  manifest.rs
  events.rs
  tar_io.rs
  limits.rs
  verify.rs
  errors.rs
```

Rules:

- `tar_io.rs`: deterministic archive encoding only.
- `limits.rs`: single source of truth for all max-size / bounded-reader policy.
- `events.rs`: NDJSON canonicalization/validation rules only.
- `mod.rs`: facade/orchestration only (no direct tar/gzip internals).

## Leak-free grep gates (for split PR)

`bundle/writer/mod.rs` no archive internals:

```bash
rg "tar::|flate2::|GzBuilder|HeaderMode|Header::new_gnu" crates/assay-evidence/src/bundle/writer/mod.rs
# Expect: 0
```

`bundle/writer/limits.rs` is sole limits owner:

```bash
rg "max_bundle_bytes|max_decode_bytes|max_manifest_bytes|max_events_bytes|max_line_bytes|max_path_len|max_json_depth|max_events" crates/assay-evidence/src/bundle/writer/{mod.rs,manifest.rs,events.rs,tar_io.rs,verify.rs}
# Expect: 0 matches outside explicit pass-through glue
```

`bundle/writer/tar_io.rs` is sole deterministic tar/gzip writer:

```bash
rg "GzBuilder::|HeaderMode::Deterministic|append_data|set_mtime\\(0\\)" crates/assay-evidence/src/bundle/writer
# Expect: matches only in tar_io.rs
```
