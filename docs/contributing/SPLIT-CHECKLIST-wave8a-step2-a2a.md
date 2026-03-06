# SPLIT CHECKLIST - Wave8A Step2 (A2A Mechanical Split)

## Scope

- [ ] Only A2A split files + Step2 docs/script changed
- [ ] No `.github/workflows/*` changes
- [ ] Split is mechanical (no behavior changes)

## Structure

- [ ] `crates/assay-adapter-a2a/src/lib.rs` is thin facade
- [ ] `adapter_impl/{convert,parse,version,fields,mapping,payload,tests}.rs` exists
- [ ] `SPLIT-MOVE-MAP-wave8a-step2-a2a.md` matches code layout

## Boundary Gates

- [ ] parse logic single-sourced in `parse.rs`
- [ ] version logic single-sourced in `version.rs`
- [ ] field extract/default-time single-sourced in `fields.rs`
- [ ] event mapping single-sourced in `mapping.rs`
- [ ] payload shaping single-sourced in `payload.rs`

## Validation

- [ ] `bash scripts/ci/review-wave8a-step2.sh` passes
- [ ] `cargo test -p assay-adapter-a2a` passes
- [ ] `bash scripts/ci/test-adapter-a2a.sh` passes
