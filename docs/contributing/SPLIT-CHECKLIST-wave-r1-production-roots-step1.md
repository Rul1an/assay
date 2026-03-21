# SPLIT CHECKLIST - Wave R1 Step1 (Production Roots)

## Scope

- [ ] Step only touches registry trust bootstrap files plus review artifacts
- [ ] No `.github/workflows/*` changes
- [ ] No dependency graph or workspace version churn

## Behavior

- [ ] `TrustStore::with_production_roots()` no longer returns an empty store
- [ ] `PackResolver::with_config()` no longer silently falls back to `TrustStore::new()`
- [ ] Invalid or empty embedded roots fail closed via `RegistryError::Config`
- [ ] Signed pack fixture resolves with embedded production roots
- [ ] Untrusted key id is rejected with a hard verification error

## Reviewer Gate

- [ ] `scripts/ci/review-wave-r1-production-roots-step1.sh` exists
- [ ] Gate runs fmt + clippy + targeted registry tests + `git diff --check`
- [ ] Gate enforces allowlist-only diff
- [ ] Gate fails if `resolver.rs` reintroduces `TrustStore::new()` in the production path
- [ ] Gate blocks workflow changes
