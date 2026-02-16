# Wave7C Step2 checklist: judge/json_strict mechanical split

Scope lock:
- Keep public signatures in `/Users/roelschuurkes/assay/crates/assay-core/src/judge/mod.rs` unchanged.
- Keep public signatures in `/Users/roelschuurkes/assay/crates/assay-evidence/src/json_strict/mod.rs` unchanged.
- Mechanical moves only: helper/orchestration extraction + facade delegation.
- No tests relocation in Step2 (tests remain in facades until Step3 closure).

Artifacts:
- `/Users/roelschuurkes/assay/docs/contributing/SPLIT-CHECKLIST-wave7c-step2-judge-json-strict.md`
- `/Users/roelschuurkes/assay/docs/contributing/SPLIT-MOVE-MAP-wave7c-step2-judge-json-strict.md`
- `/Users/roelschuurkes/assay/docs/contributing/SPLIT-REVIEW-PACK-wave7c-step2-judge-json-strict.md`
- `/Users/roelschuurkes/assay/scripts/ci/review-wave7c-step2.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7c-step2.sh
```

Hard gates (script-enforced):
- `cargo fmt --check`
- `cargo clippy -p assay-core -p assay-evidence --all-targets -- -D warnings`
- `cargo check -p assay-core -p assay-evidence`
- Step1 anchor tests remain green for judge and json_strict.
- Facade containment (code-only): ban heavy deps/IO/process/network/crypto internals in facades.
- Delegation proof: facades must call `judge_internal::run::evaluate_impl` and `json_strict_internal::run::{from_str_strict_impl,validate_json_strict_impl}`.
- Single-source boundaries:
  - `judge_internal/prompt.rs`: prompt template ownership (`SYSTEM_PROMPT`, `build_prompt_impl`).
  - `judge_internal/client.rs`: model call + parse boundary (`call_judge_impl`).
  - `judge_internal/cache.rs`: cache-key and metadata injection helpers.
  - `judge_internal/run.rs`: evaluate orchestration + rerun decision.
  - `json_strict_internal/validate.rs`: validator state machine.
  - `json_strict_internal/decode.rs`: strict string decode entrypoint.
  - `json_strict_internal/limits.rs`: limits import boundary.
- Sensitive wording tripwire in strict JSON errors.
- Strict diff allowlist.

Definition of done:
- reviewer script passes on `BASE_REF=origin/main`
- no anchor regressions
- no scope leakage outside allowlist
