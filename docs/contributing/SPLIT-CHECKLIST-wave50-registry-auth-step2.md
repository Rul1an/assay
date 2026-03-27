# Wave50 Registry Auth Step2 Checklist

- [ ] `auth.rs` reduced to a stable facade with the public auth surface and existing inline tests.
- [ ] OIDC/cache/provider internals moved under `auth_next/*`.
- [ ] `crates/assay-registry/tests/**` left untouched.
- [ ] No production edits outside `auth.rs` and `auth_next/*`.
- [ ] Reviewer script enforces scope allowlist, thin-facade markers, and no-increase drift counters.
- [ ] Local validation run:
  - `git diff --check`
  - `cargo fmt --all --check`
  - `cargo clippy -q -p assay-registry --all-targets -- -D warnings`
  - `cargo check -q -p assay-registry`
  - `cargo check -q -p assay-registry --features oidc`
  - `BASE_REF=origin/main bash scripts/ci/review-wave50-registry-auth-step2.sh`
