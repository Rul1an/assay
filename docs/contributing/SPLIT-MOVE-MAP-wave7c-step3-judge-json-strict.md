# Wave7C Step3 move-map: judge/json_strict closure

Closure changes:
- Removed `#[cfg(test)] mod tests` from:
  - `/Users/roelschuurkes/assay/crates/assay-core/src/judge/mod.rs`
  - `/Users/roelschuurkes/assay/crates/assay-evidence/src/json_strict/mod.rs`
- Relocated those test bodies into:
  - `/Users/roelschuurkes/assay/crates/assay-core/src/judge/judge_internal/tests.rs`
  - `/Users/roelschuurkes/assay/crates/assay-evidence/src/json_strict/json_strict_internal/tests.rs`

Stable facade call chains:
- `JudgeService::evaluate` -> `judge_internal::run::evaluate_impl`
- `from_str_strict` -> `json_strict_internal::run::from_str_strict_impl`
- `validate_json_strict` -> `json_strict_internal::run::validate_json_strict_impl`

Boundary model (unchanged from Step2):
- `judge_internal/prompt.rs`: prompt construction/constants only.
- `judge_internal/client.rs`: judge request/response parse only.
- `judge_internal/cache.rs`: cache key + meta injection only.
- `judge_internal/run.rs`: orchestration only.
- `json_strict_internal/validate.rs`: validator state machine only.
- `json_strict_internal/decode.rs`: strict string decode boundary only.
- `json_strict_internal/limits.rs`: strict limits boundary import only.
- `json_strict_internal/run.rs`: strict JSON facade implementation boundary only.

Step3 note:
- Public surface/signatures are unchanged; only test location and facade closure changed.
