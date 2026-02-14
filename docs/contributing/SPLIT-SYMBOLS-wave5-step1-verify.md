# Wave5 Step1 symbols (verify public surface)

Source:
- `crates/assay-registry/src/verify.rs`

Command:
```bash
rg -n "^pub (fn|struct|enum|type|const)" crates/assay-registry/src/verify.rs
```

Output:
```text
19:pub const PAYLOAD_TYPE_PACK_V1: &str = "application/vnd.assay.pack+yaml;v=1";
23:pub struct VerifyResult {
36:pub struct VerifyOptions {
72:pub fn verify_pack(
147:pub fn verify_digest(content: &str, expected: &str) -> RegistryResult<()> {
169:pub fn compute_digest(content: &str) -> String {
181:pub fn compute_digest_strict(content: &str) -> Result<String, CanonicalizeError> {
190:pub fn compute_digest_raw(content: &str) -> String {
323:pub fn compute_key_id(spki_bytes: &[u8]) -> String {
328:pub fn compute_key_id_from_key(key: &VerifyingKey) -> RegistryResult<String> {
```
