# ACP Step2 Move Map (Mechanical)

Wave14 Step2 splits `crates/assay-adapter-acp/src/lib.rs` into bounded modules with no behavior change.

## Old to new mapping

- `ProtocolAdapter::convert` body in `lib.rs`
  - moved to `crates/assay-adapter-acp/src/adapter_impl/convert.rs` as `convert_impl`
- event mapping and top-level unmapped counting
  - moved to `crates/assay-adapter-acp/src/adapter_impl/mapping.rs`
- lossiness level classification
  - moved to `crates/assay-adapter-acp/src/adapter_impl/lossiness.rs`
- payload normalization (deterministic key ordering)
  - moved to `crates/assay-adapter-acp/src/adapter_impl/normalize.rs`
- raw payload size guard + attachment writer call
  - moved to `crates/assay-adapter-acp/src/adapter_impl/raw_payload.rs`
- tests from inline `#[cfg(test)] mod tests` in `lib.rs`
  - moved to `crates/assay-adapter-acp/src/tests/mod.rs`

## Facade shape after split

- `crates/assay-adapter-acp/src/lib.rs` keeps:
  - public constants and adapter identity metadata
  - `AcpAdapter` type
  - `ProtocolAdapter` impl surface (`adapter`, `protocol`, `capabilities`, `convert`)
- `convert` method is a thin wrapper with one call to `adapter_impl::convert_impl(...)`.

## Boundary rules

- no workflow edits in Step2
- no edits outside ACP adapter tree + Step2 docs/script
- modules under `adapter_impl/*` stay internal (`pub(crate)` only)
- tests remain behavior-identical and keep the same names
