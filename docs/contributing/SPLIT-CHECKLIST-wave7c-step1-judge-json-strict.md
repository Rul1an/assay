# Wave7C Step1 Checklist: judge + json_strict freeze

Scope lock:
- docs + reviewer gates + tests only.
- no production-path code edits in:
  - `/Users/roelschuurkes/assay/crates/assay-core/src/judge/mod.rs`
  - `/Users/roelschuurkes/assay/crates/assay-evidence/src/json_strict/mod.rs`

Freeze anchors:
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

Hard gates (review-wave7c-step1.sh):
- BASE_REF guard + printed base/head SHA.
- fmt/clippy/check for `assay-core` + `assay-evidence`.
- no-production-change (code-only strip, excluding `#[cfg(test)] mod tests` blocks).
- file-local public-surface freeze.
- no-increase drift counters:
  - `unwrap/expect`, `unsafe`, print/debug/log macros,
  - `panic/todo/unimplemented`,
  - IO/process/network patterns.
- strict diff allowlist.

Definition of done:
- `BASE_REF=origin/main bash scripts/ci/review-wave7c-step1.sh` passes.
- only allowlisted files changed.
- Step1 remains behavior/perf neutral.
