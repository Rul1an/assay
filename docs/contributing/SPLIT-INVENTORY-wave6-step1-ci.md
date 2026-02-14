# Wave6 Step1 inventory: CI/CD hardening baseline

Snapshot baseline (`origin/main` at time of Step1 freeze): `5a72f04b`
PR head (Step1 change set): `40acafd4`

Scope baseline (workflows):
- `.github/workflows/split-wave0-gates.yml`
- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `.github/workflows/action-tests.yml`

Baseline anchors (present today):
- `split-wave0-gates.yml` has Wave0 feature matrix and curated feature runs.
- `split-wave0-gates.yml` uses `cargo-nextest` and `cargo-hack` on hotspot crates.
- `split-wave0-gates.yml` runs semver checks via `cargo-semver-checks`.
- `split-wave0-gates.yml` enforces anti-placeholder clippy (`todo` + `unimplemented`).
- `action-tests.yml` includes `attestation_conditional` logic test.
- `release.yml` declares `id-token: write` in release jobs.

Known gaps tracked for Wave6 follow-up (not changed in Step1):
- No explicit `attest-build-provenance` producer in release workflow.
- No CI/release attestation verification gate that fails closed.
- No dedicated nightly fuzz/model workflow lane (`miri`/fuzz/Kani) yet.

Step1 contract:
- docs + gates only.
- no workflow semantic changes.
