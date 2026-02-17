# Wave7C Step1 review pack: judge + json_strict freeze

Intent:
- Freeze behavior/contracts and reviewer gates before Wave7C mechanical split.
- Preserve public surface and production-path semantics for:
  - `crates/assay-core/src/judge/mod.rs`
  - `crates/assay-evidence/src/json_strict/mod.rs`

Executed validation:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7c-step1.sh
```

This executes:
```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo check -p assay-core -p assay-evidence
```

Anchor tests executed:
- Judge:
  - `judge::tests::contract_two_of_three_majority`
  - `judge::tests::contract_sprt_early_stop`
  - `judge::tests::contract_abstain_mapping`
  - `judge::tests::contract_determinism_parallel_replay`
- JSON strict:
  - `json_strict::tests::test_rejects_top_level_duplicate`
  - `json_strict::tests::test_rejects_unicode_escape_duplicate`
  - `json_strict::tests::test_signature_duplicate_key_attack`
  - `json_strict::tests::test_dos_nesting_depth_limit`
  - `json_strict::tests::test_string_length_over_limit_rejected`

Proof points:
- no-production-change gate compares code-only (test blocks stripped) vs `BASE_REF`.
- public-surface freeze gate compares file-local `pub` symbols vs `BASE_REF`.
- drift counters are no-increase only (best-effort code-only).
- strict allowlist prevents scope creep.

Risk:
- Low. Step1 artifacts/gates only; no mechanical movement yet.
