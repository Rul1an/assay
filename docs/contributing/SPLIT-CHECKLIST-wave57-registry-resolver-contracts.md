# Wave57 Registry Resolver Contracts Checklist

Scope:
- Add external resolver characterization tests.
- Add review gate for the pre-split resolver contract step.
- Do not edit `resolver.rs` in this PR.

Required contracts:
- `resolver_uses_fresh_cache_before_network`
- `resolver_evicts_pinned_cache_mismatch_and_refetches`
- `resolver_no_cache_skips_cached_entry_and_fetches_registry`

Required checks:
- `cargo fmt --check`
- `cargo check -p assay-registry`
- `cargo test -p assay-registry resolver`
- `cargo test -p assay-registry --test resolver_contracts`
- `cargo clippy -p assay-registry --all-targets -- -D warnings`
- `BASE_REF=origin/main bash scripts/ci/review-wave57-registry-resolver-contracts.sh`

Non-goals:
- No `resolver_next` module creation.
- No `resolver.rs` code movement.
- No cache, trust, client, or verification behavior changes.
- No dependency or workflow changes.
