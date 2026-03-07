# ACP Step2 Review Pack (Mechanical Split)

## Intent

Perform the Wave14 mechanical split of `assay-adapter-acp` while keeping public behavior and adapter contract unchanged.

## Scope

- `crates/assay-adapter-acp/src/lib.rs`
- `crates/assay-adapter-acp/src/adapter_impl/mod.rs`
- `crates/assay-adapter-acp/src/adapter_impl/convert.rs`
- `crates/assay-adapter-acp/src/adapter_impl/mapping.rs`
- `crates/assay-adapter-acp/src/adapter_impl/lossiness.rs`
- `crates/assay-adapter-acp/src/adapter_impl/normalize.rs`
- `crates/assay-adapter-acp/src/adapter_impl/raw_payload.rs`
- `crates/assay-adapter-acp/src/tests/mod.rs`
- `docs/contributing/SPLIT-CHECKLIST-acp-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-acp-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-acp-step2.md`
- `scripts/ci/review-acp-step2.sh`

## Non-goals

- no workflow changes
- no API redesign
- no protocol semantics changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-acp-step2.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-adapter-acp -p assay-adapter-api --all-targets -- -D warnings
cargo test -p assay-adapter-acp
```

## Reviewer 60s scan

1. Confirm diff stays in Step2 allowlist.
2. Confirm `lib.rs` is thin and has one `adapter_impl::convert_impl(...)` call.
3. Confirm no inline test module in `lib.rs`.
4. Confirm `adapter_impl/*` exports are `pub(crate)` only.
5. Run reviewer script and expect PASS.
