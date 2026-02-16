# Wave7C Step1 Symbols: judge + json_strict

Source command:
```bash
rg -n '^\s*pub\s+(const|struct|enum|type|trait|fn)\b' \
  crates/assay-core/src/judge/mod.rs \
  crates/assay-evidence/src/json_strict/mod.rs
```

Output snapshot:
```text
crates/assay-core/src/judge/mod.rs:9:pub struct JudgeRuntimeConfig {
crates/assay-core/src/judge/mod.rs:27:pub struct JudgeService {
crates/assay-core/src/judge/mod.rs:35:    pub fn new(
crates/assay-evidence/src/json_strict/mod.rs:73:pub fn from_str_strict<T: DeserializeOwned>(s: &str) -> Result<T, StrictJsonError> {
crates/assay-evidence/src/json_strict/mod.rs:83:pub fn validate_json_strict(s: &str) -> Result<(), StrictJsonError> {
```

Notes:
- This freeze is file-local public-surface parity for the two hotspots.
- No crate-level API redesign is in scope for Step1.
