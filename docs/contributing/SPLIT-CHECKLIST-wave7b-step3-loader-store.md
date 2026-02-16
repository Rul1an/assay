# Wave7B Step3 checklist: loader/store closure

Scope lock:
- Keep public signatures in `/Users/roelschuurkes/assay/crates/assay-evidence/src/lint/packs/loader.rs` unchanged.
- Keep public signatures in `/Users/roelschuurkes/assay/crates/assay-core/src/storage/store.rs` unchanged.
- Finalize closure boundaries:
  - `loader.rs` becomes testless thin facade.
  - loader unit tests move to `loader_internal/tests.rs`.
  - store Step2 helper boundaries remain enforced.

Artifacts:
- `/Users/roelschuurkes/assay/docs/contributing/SPLIT-CHECKLIST-wave7b-step3-loader-store.md`
- `/Users/roelschuurkes/assay/docs/contributing/SPLIT-MOVE-MAP-wave7b-step3-loader-store.md`
- `/Users/roelschuurkes/assay/docs/contributing/SPLIT-REVIEW-PACK-wave7b-step3-loader-store.md`
- `/Users/roelschuurkes/assay/scripts/ci/review-wave7b-step3.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7b-step3.sh
```

Hard gates (script-enforced):
- `cargo fmt --check`
- `cargo clippy -p assay-evidence -p assay-core --all-targets -- -D warnings`
- `cargo check -p assay-evidence -p assay-core`
- Loader and store Step1 anchor tests remain green.
- Loader facade:
  - no `#[cfg(test)]`
  - no `mod tests`
  - no private `fn` definitions
  - delegates only via `loader_internal::run::*`.
- Tests single-source:
  - loader test fns live in `loader_internal/tests.rs`.
- Store single-source helper boundaries from Step2 remain intact.
- Strict diff allowlist.

Definition of done:
- reviewer script passes on `BASE_REF=origin/main`
- loader facade is thin and testless
- contract anchors run and pass with real test execution
