# Writer split Step 3 move map

Scope: `crates/assay-evidence/src/bundle/writer.rs` -> `crates/assay-evidence/src/bundle/writer_next/*`.

Status:
- Commit A: scaffold only, no function moves.
- Commit B: populate this map with exact 1:1 moves.

## Public surface freeze (must remain stable)

The following public symbols/signatures are the contract surface to preserve through Step 3:

- `pub struct Manifest`
- `pub struct AlgorithmMeta`
- `pub struct FileMeta`
- `pub struct BundleWriter<W: Write>`
- `impl BundleWriter<W>` methods:
  - `pub fn new(writer: W) -> Self`
  - `pub fn with_producer(mut self, producer: ProducerMeta) -> Self`
  - `pub fn add_event(&mut self, event: EvidenceEvent)`
  - `pub fn add_events(&mut self, events: impl IntoIterator<Item = EvidenceEvent>)`
  - `pub fn finish(mut self) -> Result<()>`
- `pub struct VerifyResult`
- `pub fn verify_bundle<R: Read>(reader: R) -> Result<VerifyResult>`
- `pub fn verify_bundle_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<VerifyResult>`
- `pub enum ErrorClass`
- `pub enum ErrorCode`
- `pub struct VerifyError`
- `impl VerifyError` methods:
  - `pub fn new(class: ErrorClass, code: ErrorCode, message: impl Into<String>) -> Self`
  - `pub fn with_source(mut self, source: impl Into<anyhow::Error>) -> Self`
  - `pub fn with_context(mut self, context: impl Into<String>) -> Self`
  - `pub fn class(&self) -> ErrorClass`
- `pub struct VerifyLimits`
- `impl Default for VerifyLimits`
- `pub struct VerifyLimitsOverrides`
- `impl VerifyLimits` method:
  - `pub fn apply(self, overrides: VerifyLimitsOverrides) -> Self`

## Move map table (populate in Commit B)

| Old symbol (writer.rs) | New file | Notes |
|---|---|---|
| _TBD in Commit B_ | _TBD_ | _Mechanical 1:1 move only_ |
