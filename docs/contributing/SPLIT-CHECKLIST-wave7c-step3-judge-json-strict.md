# Wave7C Step3 checklist: judge/json_strict closure

Scope lock:
- Keep public signatures in `crates/assay-core/src/judge/mod.rs` unchanged.
- Keep public signatures in `crates/assay-evidence/src/json_strict/mod.rs` unchanged.
- Close the split by making both facades testless and delegation-only.

Artifacts:
- `docs/contributing/SPLIT-CHECKLIST-wave7c-step3-judge-json-strict.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave7c-step3-judge-json-strict.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave7c-step3-judge-json-strict.md`
- `scripts/ci/review-wave7c-step3.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7c-step3.sh
```

Hard gates (script-enforced):
- `cargo fmt --check`
- `cargo clippy -p assay-core -p assay-evidence --all-targets -- -D warnings`
- `cargo check -p assay-core -p assay-evidence`
- Step1 anchors remain green at relocated paths:
  - `judge::judge_internal::tests::*`
  - `json_strict::json_strict_internal::tests::*`
- Facade closure:
  - no `#[cfg(test)]` and no `mod tests` in either facade
  - no private `fn` definitions in either facade
  - delegation entrypoints present (`evaluate_impl`, `from_str_strict_impl`, `validate_json_strict_impl`)
- Single-source boundaries remain enforced for prompt/client/cache/run + validate/decode/limits/run.
- Sensitive strict-JSON wording tripwires remain stable.
- Strict diff allowlist.

Definition of done:
- reviewer script passes on `BASE_REF=origin/main`
- facades are testless and delegation-only
- no allowlist leaks
