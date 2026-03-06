# SPLIT MOVE MAP - Wave T1 B1 (parity.rs)

## Section Move Map

- `parity.rs` header/docs -> `parity.rs` (facade kept)
- Core type declarations -> `parity/core_types.rs`
- Shared evaluation logic -> `parity/shared.rs`
- Batch engine path -> `parity/batch.rs`
- Streaming engine path -> `parity/streaming.rs`
- Parity assertion/hash path -> `parity/assertions.rs`
- Fixture builders -> `parity/fixtures.rs`
- Test functions -> `parity/parity_contract.rs`

## Symbol Map (old -> new)

- `compute_result_hash` -> `parity/assertions.rs`
- `verify_parity` -> `parity/assertions.rs`
- `ParityResult` -> `parity/assertions.rs`
- `batch::evaluate` -> `parity/batch.rs`
- `streaming::evaluate` -> `parity/streaming.rs`
- `shared::{args_valid, sequence_valid, blocklist}` -> `parity/shared.rs`
- `fixtures::all_test_cases` -> `parity/fixtures.rs`
- Contract tests (`test_*`) -> `parity/parity_contract.rs`

## Facade Contract

- `crates/assay-core/tests/parity.rs` keeps public module surface and re-exports:
  - `CheckInput`, `CheckResult`, `CheckType`, `Outcome`, `PolicyCheck`, `ToolCall`
  - `compute_result_hash`, `verify_parity`, `ParityResult`

This preserves test-target entrypoint and call sites under `cargo test -p assay-core --test parity`.
