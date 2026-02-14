# Wave6 Step3 inventory: nightly safety lane

Snapshot baseline (`origin/main` before Step3): `3e479c88`
Working branch head: see `git rev-parse --short HEAD`

Target files:
- `.github/workflows/wave6-nightly-safety.yml`
- `scripts/ci/review-wave6-step3-ci.sh`
- `docs/contributing/SPLIT-*wave6-step3-nightly.md`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Step3 contract:
- add non-blocking nightly/model lane (schedule + manual trigger)
- keep PR-required CI paths unchanged
- keep Wave0/Step2 gates intact

Nightly anchors (this step):
- miri smoke: `cargo miri test -p assay-registry test_verify_pack_fail_closed_matrix_contract`
- property smoke: `cargo test -p assay-cli test_roundtrip_property`
- `continue-on-error: true` on smoke jobs

Non-goals:
- no required-status promotion in Step3
- no Kani lane in this step
