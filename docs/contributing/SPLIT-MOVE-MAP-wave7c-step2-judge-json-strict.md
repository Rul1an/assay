# Wave7C Step2 move-map: judge/json_strict

Boundary legend:
- `judge_internal/run.rs`: judge orchestration and public evaluate implementation.
- `judge_internal/client.rs`: LLM client call + response parse boundary.
- `judge_internal/prompt.rs`: prompt construction and prompt marker constant.
- `judge_internal/cache.rs`: cache key generation + metadata injection helpers.
- `json_strict_internal/run.rs`: strict JSON public entrypoint implementations.
- `json_strict_internal/validate.rs`: validator state machine and recursive object/array validation.
- `json_strict_internal/decode.rs`: string decode wrapper over scanner.
- `json_strict_internal/limits.rs`: strict limits constants boundary import.

Moved functions -> target file:
- `JudgeService::evaluate` body delegates to `judge_internal::run::evaluate_impl`.
- `call_judge` helper -> `judge_internal::client::call_judge_impl`
- `build_prompt` helper -> `judge_internal::prompt::build_prompt_impl`
- `generate_cache_key` helper -> `judge_internal::cache::generate_cache_key_impl`
- `inject_result` helper -> `judge_internal::cache::inject_result_impl`
- `from_str_strict` body delegates to `json_strict_internal::run::from_str_strict_impl`
- `validate_json_strict` body delegates to `json_strict_internal::run::validate_json_strict_impl`
- `JsonValidator` + recursive validation methods -> `json_strict_internal/validate.rs`
- strict string parser boundary -> `json_strict_internal/decode.rs::parse_json_string_impl`

Facade call chains (current):
- `JudgeService::evaluate` -> `judge_internal::run::evaluate_impl` -> `prompt::build_prompt_impl` -> `client::call_judge_impl` -> `cache::{generate_cache_key_impl,inject_result_impl}`
- `from_str_strict` -> `json_strict_internal::run::from_str_strict_impl` -> `validate::JsonValidator::validate` -> `serde_json::from_str`
- `validate_json_strict` -> `json_strict_internal::run::validate_json_strict_impl` -> `validate::JsonValidator::validate`

Step2 note:
- Tests intentionally remain in `crates/assay-core/src/judge/mod.rs` and `crates/assay-evidence/src/json_strict/mod.rs`; relocation is deferred to Step3 closure.
