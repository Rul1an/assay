# SPLIT CHECKLIST - Wave8B Step2 (UCP Mechanical Split)

## Scope

- [ ] Only UCP split files + Step2 docs/script changed
- [ ] No `.github/workflows/*` changes
- [ ] Split is mechanical (no behavior changes)

## Structure

- [ ] `crates/assay-adapter-ucp/src/lib.rs` is thin facade
- [ ] `adapter_impl/{convert,parse,version,fields,mapping,payload,tests}.rs` exists
- [ ] `SPLIT-MOVE-MAP-wave8b-step2-ucp.md` matches code layout

## Boundary Gates

- [ ] parse logic single-sourced in `parse.rs`
- [ ] version logic single-sourced in `version.rs`
- [ ] field extract/default-time single-sourced in `fields.rs`
- [ ] event mapping single-sourced in `mapping.rs`
- [ ] payload shaping single-sourced in `payload.rs`

## Validation

- [ ] `bash scripts/ci/review-wave8b-step2.sh` passes
- [ ] `cargo test -p assay-adapter-ucp` passes
- [ ] `bash scripts/ci/test-adapter-ucp.sh` passes
